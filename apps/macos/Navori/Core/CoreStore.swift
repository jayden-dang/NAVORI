import Foundation
import NavoriCore
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
            homeDir: NSHomeDirectory(),
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
            startupError = String(describing: error)
            log.error("core failed to start: \(String(describing: error), privacy: .public)")
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

    var health: Health {
        if let startupError { return .down(startupError) }
        guard let snapshot else { return .starting }
        if let failed = snapshot.modules.first(where: { $0.state == "failed" }) {
            return .degraded("\(failed.id) stopped: \(failed.detail ?? "unknown error")")
        }
        if let restarting = snapshot.modules.first(where: { $0.state == "restarting" }) {
            return .degraded("\(restarting.id) is restarting")
        }
        if let configError = snapshot.configError {
            return .degraded("config.toml is invalid — using defaults. \(configError)")
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
