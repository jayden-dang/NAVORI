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
    private(set) var snapshot: CoreSnapshot?
    private(set) var startupError: String?
    private(set) var checks: [CheckView] = []
    private(set) var isRunningChecks = false

    @ObservationIgnored private var core: NavoriCore?
    @ObservationIgnored private let log = Logger(subsystem: "dev.navori", category: "core")
    private var awakeActionFailure: AwakeActionFailure?

    func start() {
        guard core == nil else { return }
        let config = CoreConfig(
            profile: AppEnvironment.profile,
            homeDir: AppEnvironment.coreHome,
            uid: UInt32(getuid()),
            appVersion: AppEnvironment.version,
            logToFile: true,
            helperPath: AppEnvironment.cliURL.path
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

    func saveProviderKey(_ key: String) async throws {
        guard let core else { throw StoreError.notRunning }
        do {
            try await core.saveProviderKey(key: key)
        } catch CoreError.Secrets(let message) {
            throw StoreError.keychain(message)
        }
    }

    func removeProviderKey() async throws -> Bool {
        guard let core else { throw StoreError.notRunning }
        do {
            return try await core.removeProviderKey()
        } catch CoreError.Secrets(let message) {
            throw StoreError.keychain(message)
        }
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
        CoreStatus(health: CoreHealth(
            coreFailed: startupError != nil,
            hasSnapshot: snapshot != nil,
            moduleStates: snapshot?.modules.map(\.state) ?? [],
            hasConfigNotice: snapshot?.configNotice != nil
        ))
    }

    /// The status item's one value. The only reader of `snapshot.tray`.
    var statusIcon: StatusIconState {
        StatusIconState(
            coreFailed: startupError != nil,
            core: snapshot.map { snapshot in
                StatusIconState(
                    glyph: Self.glyph(snapshot.tray.glyph),
                    attention: snapshot.tray.attention,
                    description: snapshot.tray.summary
                )
            }
        )
    }

    private static func glyph(_ glyph: TrayGlyphView) -> StatusIconGlyph {
        switch glyph {
        case .normal: .normal
        case .lease: .lease
        case .aiRunning: .aiRunning
        case .problem: .problem
        }
    }

    /// Missing record, or the module stopped, is `Unavailable`. A lease with no system assertion is `Not held`.
    var awakeStatus: AwakeStatus {
        guard let awake = snapshot?.awake else { return .unavailable }
        return AwakeLevel0.row(
            problem: awake.problem,
            systemHeld: awake.systemHeld,
            hasLease: awake.leases.isEmpty == false,
            sources: awake.leases.compactMap {
                AwakeLeaseMap.sourceName(kind: $0.kind, harness: $0.harness, label: $0.label)
            }
        )
    }

    /// The failed menu action still on screen, if a later snapshot has not achieved it.
    var awakeActionMessage: String? {
        awakeActionFailure?.visible(against: awakeMarks)
    }

    private var awakeMarks: AwakeHeldMarks {
        Self.marks(of: snapshot?.awake)
    }

    private static func marks(of awake: AwakeView?) -> AwakeHeldMarks {
        let leases = awake?.leases ?? []
        return AwakeHeldMarks(
            durationEndsAtMs: leases.first { $0.kind == "duration" }?.endsAtMs,
            hasUntilOff: leases.contains { $0.kind == "until_off" },
            hasDisplay: leases.contains { $0.kind == "display" },
            tracking: awake?.tracking ?? true
        )
    }

    var awakeLeases: [AwakeLeaseInput] {
        snapshot?.awake?.leases.compactMap { lease in
            AwakeLeaseMap.lease(
                kind: lease.kind,
                harness: lease.harness,
                label: lease.label,
                pid: lease.pid,
                stopped: lease.stopped,
                endsAtMs: lease.endsAtMs,
                title: lease.title
            )
        } ?? []
    }

    var awakeSubmenu: AwakeSubmenuState {
        AwakeSubmenuState(
            status: awakeStatus,
            leases: awakeLeases,
            displayHeld: snapshot?.awake?.displayHeld ?? false,
            tracking: snapshot?.awake?.tracking ?? true,
            actionFailure: awakeActionMessage
        )
    }

    func perform(_ command: AwakeCommand) async {
        do {
            _ = try await dispatch(module: "awake", actionJSON: command.actionJSON)
            awakeActionFailure = nil
        } catch {
            let message = CoreStartupBanner.menuFailure(for: error)
            log.error("awake command failed: \(message, privacy: .public)")
            awakeActionFailure = AwakeActionFailure(
                request: command,
                message: message,
                marks: awakeMarks
            )
        }
    }

    fileprivate func apply(_ snapshot: CoreSnapshot) {
        self.snapshot = snapshot
        if let failure = awakeActionFailure, failure.visible(against: Self.marks(of: snapshot.awake)) == nil {
            awakeActionFailure = nil
        }
    }

    enum StoreError: LocalizedError {
        case notRunning
        case keychain(String)

        var errorDescription: String? {
            switch self {
            case .notRunning: "The Navori core is not running."
            case .keychain(let message): message
            }
        }
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
