import Foundation
import NavoriBanner
import Observation
import OSLog

struct IntegrationRow: Decodable, Equatable, Identifiable {
    let harness: String
    let path: String
    let state: String
    let reason: String?
    let offers: [String]

    var id: String { harness }

    var title: String {
        switch harness {
        case "claude": "Claude Code"
        case "codex": "Codex"
        case "grok": "Grok Build"
        default: harness
        }
    }

    var stateLabel: String {
        switch state {
        case "not_found": "Not found"
        case "not_installed": "Not installed"
        case "installed": "Installed"
        case "needs_repair": "Needs repair"
        case "unreadable": "Unreadable"
        default: state
        }
    }

    var symbol: String {
        switch state {
        case "installed": "checkmark.circle.fill"
        case "needs_repair": "exclamationmark.triangle.fill"
        case "unreadable": "xmark.octagon.fill"
        case "not_installed": "circle.dashed"
        default: "questionmark.circle"
        }
    }

    func offers(_ action: IntegrationsModel.Action) -> Bool { offers.contains(action.rawValue) }
}

private struct IntegrationsReply: Decodable {
    let harnesses: [IntegrationRow]
}

@MainActor
@Observable
final class IntegrationsModel {
    enum Action: String {
        case install, repair, remove
    }

    private(set) var rows: [IntegrationRow] = []
    private(set) var error: String?
    private(set) var isBusy = false

    @ObservationIgnored private let store: CoreStore
    @ObservationIgnored private let log = Logger(subsystem: "dev.navori", category: "settings")

    init(store: CoreStore) {
        self.store = store
    }

    func refresh() async {
        guard !isBusy else { return }
        isBusy = true
        defer { isBusy = false }
        await load()
    }

    func perform(_ action: Action, harness: String) async {
        guard !isBusy else { return }
        isBusy = true
        defer { isBusy = false }
        do {
            _ = try await store.dispatch(
                module: "integrations",
                actionJSON: "{\"\(action.rawValue)\":{\"harness\":\"\(harness)\"}}"
            )
            error = nil
        } catch {
            report(error, doing: action.rawValue)
        }
        await load()
    }

    private func load() async {
        do {
            let reply = try await store.dispatch(module: "integrations", actionJSON: "\"status\"")
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            rows = try decoder.decode(IntegrationsReply.self, from: Data(reply.utf8)).harnesses
            error = nil
        } catch {
            report(error, doing: "status")
        }
    }

    private func report(_ failure: Error, doing what: String) {
        let message = CoreStartupBanner.menuFailure(for: failure)
        log.error("integrations \(what, privacy: .public) failed: \(message, privacy: .public)")
        error = message
    }
}
