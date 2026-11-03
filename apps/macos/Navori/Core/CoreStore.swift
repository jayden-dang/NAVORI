import Foundation
import NavoriBanner
import NavoriCore
import NavoriPanel
import Observation
import os

/// The single bridge between SwiftUI/AppKit and the Rust core.
///
/// Everything here runs on the main actor. Snapshots arrive on a Rust worker thread through
/// `SnapshotRelay` and are hopped to the main actor before they touch observable state.
@MainActor
@Observable
final class CoreStore {
    enum Health: Equatable {
        case starting
        case ok
        case degraded(String)
        case down(String)
    }

    private(set) var snapshot: CoreSnapshot?
    private(set) var startupError: String?
    private(set) var checks: [CheckView] = []
    private(set) var isRunningChecks = false

    @ObservationIgnored private var core: NavoriCore?
    @ObservationIgnored private let log = Logger(subsystem: "dev.navori", category: "core")

    func start() {
        guard core == nil else { return }
        let config = CoreConfig(
            profile: AppEnvironment.profile,
            homeDir: AppEnvironment.coreHome,
            uid: UInt32(getuid()),
            appVersion: AppEnvironment.version,
            logToFile: true
        )
        do {
            let core = try NavoriCore.start(config: config, listener: SnapshotRelay(store: self))
            self.core = core
            self.snapshot = core.status()
            startupError = nil
            log.info("core started")
        } catch {
            let message = CoreStartupBanner.message(for: error)
            startupError = message
            log.error("core failed to start: \(message, privacy: .public)")
        }
    }

    /// Blocks briefly while modules stop. Call from `applicationWillTerminate`.
    func stop() {
        core?.shutdown()
        core = nil
    }

    func setPanelVisible(_ visible: Bool) {
        core?.setPanelVisible(visible: visible)
    }

    func systemDidWake() {
        core?.systemDidWake()
    }

    func runChecks() {
        guard let core, !isRunningChecks else { return }
        isRunningChecks = true
        Task.detached(priority: .userInitiated) { [weak self] in
            let checks = core.doctor()
            await MainActor.run {
                self?.checks = checks
                self?.isRunningChecks = false
            }
        }
    }

    func dispatch(module: String, actionJSON: String) async throws -> String {
        guard let core else { throw StoreError.notRunning }
        return try await core.dispatch(module: module, actionJson: actionJSON)
    }

    var moduleCount: Int {
        snapshot?.modules.count(where: { $0.state == "running" }) ?? 0
    }

    var moduleRows: [ModuleRow] {
        snapshot?.modules.map {
            ModuleRow(id: $0.id, state: $0.state, detail: $0.detail, restarts: $0.restarts)
        } ?? []
    }

    var eventRows: [EventRow] {
        snapshot?.recentEvents.map {
            EventRow(id: $0.id, tsMs: $0.tsMs, severity: $0.severity, summary: $0.summary)
        } ?? []
    }

    var uncleanExit: String? { snapshot?.lastUncleanExit }

    var configProblem: ConfigProblem? {
        guard let notice = snapshot?.configNotice else { return nil }
        return ConfigProblem(title: notice.title, message: notice.message)
    }

    var hasSnapshot: Bool { snapshot != nil }

    var coreStatus: CoreStatus {
        switch health {
        case .starting: CoreStatus(health: .starting)
        case .ok: CoreStatus(health: .ok)
        case .degraded: CoreStatus(health: .degraded)
        case .down: CoreStatus(health: .down)
        }
    }

    /// Missing record, or the module stopped, is `Unavailable`. A lease with no system assertion is `Not held`.
    var awakeStatus: AwakeStatus {
        guard let awake = snapshot?.awake else { return .unavailable }
        if awake.problem == Self.moduleStopped { return .unavailable }
        if awake.leases.isEmpty { return .off }
        guard awake.systemHeld else { return .notHeld }
        return .on(sources: awake.leases.compactMap(awakeSourceName))
    }

    private static let moduleStopped = "The Awake module stopped."

    /// Session rows name the harness only. Other leases use the label the module already published.
    private func awakeSourceName(_ lease: LeaseView) -> String? {
        let name = lease.kind == "session" ? lease.harness : lease.label
        guard let name, name.isEmpty == false else { return nil }
        return name
    }

    var awakeBlockStatus: AwakeBlockStatus {
        switch awakeStatus {
        case .off: .off
        case .notHeld: .notHeld
        case .unavailable: .unavailable
        case .on:
            .on(since: snapshot?.awake?.sinceMs.map(Self.clockText) ?? "")
        }
    }

    var awakeLeases: [AwakeLeaseInput] {
        snapshot?.awake?.leases.compactMap(awakeLeaseInput) ?? []
    }

    var displayHeld: Bool { snapshot?.awake?.displayHeld ?? false }

    var displayWanted: Bool {
        snapshot?.awake?.leases.contains { $0.kind == "display" } ?? false
    }

    func holdOneHour() async {
        let seconds = AwakeSubmenuState.menuHoldSeconds
        await performAwake("{\"hold_for\":{\"seconds\":\(seconds)}}")
    }

    func holdUntilOff() async {
        await performAwake("\"hold_until_off\"")
    }

    func setDisplay(on: Bool) async {
        await performAwake("{\"display\":{\"on\":\(on)}}")
    }

    func releaseAll() async {
        await performAwake("\"release_all\"")
    }

    private func performAwake(_ actionJSON: String) async {
        do {
            _ = try await dispatch(module: "awake", actionJSON: actionJSON)
        } catch {
            log.error("awake command failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    private func awakeLeaseInput(_ lease: LeaseView) -> AwakeLeaseInput? {
        let kind: AwakeLeaseInput.Kind
        switch lease.kind {
        case "duration": kind = .duration
        case "until_off": kind = .untilOff
        case "display": kind = .display
        case "command": kind = .command
        case "session": kind = .session
        default: return nil
        }
        let until = kind == .duration ? lease.endsAtMs.map(Self.clockText) : nil
        return AwakeLeaseInput(
            kind: kind,
            harness: nonempty(lease.harness),
            label: nonempty(lease.label),
            pid: lease.pid,
            stopped: lease.stopped,
            until: until
        )
    }

    private func nonempty(_ value: String?) -> String? {
        guard let value, value.isEmpty == false else { return nil }
        return value
    }

    private static func clockText(_ ms: Int64) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.timeZone = .current
        formatter.dateFormat = "HH:mm"
        return formatter.string(from: Date(timeIntervalSince1970: TimeInterval(ms) / 1000))
    }

    var health: Health {
        if let startupError { return .down(startupError) }
        guard let snapshot else { return .starting }
        if let failed = snapshot.modules.first(where: { $0.state == "failed" }) {
            return .degraded("\(failed.id) stopped: \(failed.detail ?? "unknown error")")
        }
        if let restarting = snapshot.modules.first(where: { $0.state == "restarting" }) {
            return .degraded("\(restarting.id) is restarting")
        }
        if let notice = snapshot.configNotice {
            return .degraded("\(notice.title). \(notice.message)")
        }
        return .ok
    }

    fileprivate func apply(_ snapshot: CoreSnapshot) {
        self.snapshot = snapshot
    }

    enum StoreError: LocalizedError {
        case notRunning
        var errorDescription: String? { "The Navori core is not running." }
    }
}

struct ModuleRow: Identifiable, Equatable {
    let id: String
    let state: String
    let detail: String?
    let restarts: UInt32
}

struct EventRow: Identifiable, Equatable {
    let id: String
    let tsMs: Int64
    let severity: String
    let summary: String
}

struct ConfigProblem: Equatable {
    let title: String
    let message: String
}

/// Receives snapshots on a core worker thread and forwards them to the main actor.
final class SnapshotRelay: SnapshotListener, @unchecked Sendable {
    // Only read here to capture into the main-actor task; never dereferenced off the main actor.
    private weak var store: CoreStore?

    init(store: CoreStore) {
        self.store = store
    }

    func onSnapshot(snapshot: CoreSnapshot) {
        Task { @MainActor [weak store] in
            store?.apply(snapshot)
        }
    }
}
