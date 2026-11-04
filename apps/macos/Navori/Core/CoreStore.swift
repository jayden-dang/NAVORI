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
    private(set) var stopFlows: [String: StopFlow] = [:]
    private(set) var stopProgress = StopProgress()

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
        if visible == false { dismissAskingStops() }
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
        try await core.saveProviderKey(key: key)
    }

    func removeProviderKey() async throws -> Bool {
        guard let core else { throw StoreError.notRunning }
        return try await core.removeProviderKey()
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

    var running: RunningInput? {
        guard let view = snapshot?.running else { return nil }
        return RunningInput(
            unknownReason: view.unknownReason,
            projects: view.projects.map(Self.input),
            hiddenSystemPorts: Int(view.hiddenSystemPorts),
            stopAll: view.stopAll,
            stopAllPorts: Int(view.stopAllPorts),
            inUsePorts: Int(view.inUsePorts),
            otherPortsLeft: Int(view.otherPortsLeft)
        )
    }

    var runningSubmenu: RunningSubmenuState? {
        running.map { RunningSubmenuState(running: $0, problem: awakeSubmenu.problem?.title) }
    }

    var runningRowIds: [String] {
        guard let state = runningSubmenu else { return [] }
        return state.rows.map(\.id) + (state.stopAll == nil ? [] : [Self.stopAllId])
    }

    static let stopAllId = "running.stopAll"

    func stopFlow(_ owner: String) -> StopFlow {
        stopFlows[owner] ?? .idle
    }

    func askStop(_ owner: String, count: Int) {
        var flow = stopFlow(owner)
        flow.press(count: count)
        stopFlows[owner] = flow
    }

    func dismissStop(_ owner: String) {
        var flow = stopFlow(owner)
        flow.dismiss()
        stopFlows[owner] = flow
    }

    func dismissAskingStops() {
        for (owner, flow) in stopFlows {
            if case .asking = flow { dismissStop(owner) }
        }
    }

    var isAskingToStop: Bool {
        stopFlows.values.contains { if case .asking = $0 { true } else { false } }
    }

    /// Sends the Core's own request, only if the confirmation for `owner` still matches the offer shown now.
    func confirmStop(_ owner: String, offer: StopOffer) {
        var flow = stopFlow(owner)
        run(owner, flow.confirm(offer), flow: flow)
    }

    func confirmStop(_ owner: String, offer: StopAllOffer) {
        var flow = stopFlow(owner)
        run(owner, flow.confirm(offer), flow: flow)
    }

    private func run(_ owner: String, _ toSend: [String], flow: StopFlow) {
        stopFlows[owner] = flow
        guard toSend.isEmpty == false else { return }
        Task { @MainActor [weak self] in
            guard let self else { return }
            let results = await stop(toSend)
            var flow = stopFlow(owner)
            flow.finish(results)
            stopFlows[owner] = flow
            stopProgress.record(results)
            pruneStopState()
        }
    }

    func stop(_ requests: [String]) async -> [StopResult] {
        let sends = requests.map { request in Task { @MainActor in await self.send(request) } }
        var all: [StopResult] = []
        for send in sends { all += await send.value }
        return all
    }

    private func send(_ request: String) async -> [StopResult] {
        do {
            let reply = try await dispatch(module: "ports", actionJSON: request)
            let results = try JSONDecoder().decode(KillReply.self, from: Data(reply.utf8)).results
            return results.isEmpty ? [.unanswered("Navori got no answer to the stop.")] : results
        } catch {
            let message = CoreStartupBanner.menuFailure(for: error)
            log.error("stop failed: \(message, privacy: .public)")
            return [.unanswered(message)]
        }
    }

    private var listedTargets: Set<StopTarget> {
        Set(running?.projects.flatMap { $0.ports.compactMap(\.target) } ?? [])
    }

    private func pruneStopState() {
        stopProgress.prune(listed: listedTargets)
        let live = Set(runningRowIds)
        let offers = runningSubmenu.map(Self.offerCounts) ?? [:]
        for (owner, flow) in stopFlows {
            switch flow {
            case .failed where live.contains(owner) == false:
                stopFlows[owner] = .idle
            case .asking(let count) where offers[owner] != count:
                stopFlows[owner] = .idle
            default:
                break
            }
        }
    }

    private static func offerCounts(_ state: RunningSubmenuState) -> [String: Int] {
        var counts: [String: Int] = [:]
        for row in state.rows {
            if let stop = row.stop { counts[row.id] = stop.count }
        }
        if let all = state.stopAll { counts[stopAllId] = all.ports }
        return counts
    }

    private static func input(_ state: ProjectState) -> RunningProjectState {
        switch state {
        case .working: .working
        case .idle: .idle
        case .unknown: .unknown
        }
    }

    private static func input(_ target: StopTargetView) -> StopTarget {
        switch target {
        case let .process(pid, startUs): .process(pid: pid, startUs: startUs)
        case let .container(id): .container(id: id)
        }
    }

    private static func input(_ project: RunningProject) -> RunningProjectInput {
        RunningProjectInput(
            name: project.name,
            isOther: project.isOther,
            state: Self.input(project.state),
            sessions: project.sessions.map { RunningSessionInput(stopped: $0.stopped) },
            ports: project.ports.map {
                RunningPortInput(
                    port: Int($0.port),
                    name: $0.name,
                    stopRequest: $0.stopRequest,
                    target: Self.input($0.target)
                )
            },
            stoppablePorts: Int(project.stoppablePorts),
            rowStopRequest: project.rowStopRequest,
            childStopRequest: project.childStopRequest
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
        pruneStopState()
        if let failure = awakeActionFailure, failure.visible(against: Self.marks(of: snapshot.awake)) == nil {
            awakeActionFailure = nil
        }
    }

    enum StoreError: LocalizedError {
        case notRunning

        var errorDescription: String? {
            switch self {
            case .notRunning: "The Navori core is not running."
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
