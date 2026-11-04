import Foundation

public enum StopOutcome: String, Decodable, Equatable, Sendable {
    case stopped
    case stoppedAlone = "stopped_alone"
    case alreadyGone = "already_gone"
    case stillStopping = "still_stopping"
    case stale
    case protected
    case outsideSelector = "outside_selector"
    case failed

    var clearsRow: Bool {
        switch self {
        case .stopped, .stoppedAlone, .alreadyGone: true
        default: false
        }
    }

    var fallbackReason: String {
        switch self {
        case .stillStopping: "Still stopping."
        case .stale: "The process changed."
        case .protected: "This process is protected."
        case .outsideSelector: "This port is outside the request."
        case .failed: "The stop failed."
        case .stopped, .stoppedAlone, .alreadyGone: ""
        }
    }
}

public enum StopTarget: Decodable, Hashable, Sendable {
    case process(pid: Int32, startUs: UInt64)
    case container(id: String)

    private enum Key: String, CodingKey { case process, container }
    private enum ProcessKey: String, CodingKey { case pid, startUs = "start_us" }
    private enum ContainerKey: String, CodingKey { case id }

    public init(from decoder: Decoder) throws {
        let outer = try decoder.container(keyedBy: Key.self)
        if outer.contains(.process) {
            let inner = try outer.nestedContainer(keyedBy: ProcessKey.self, forKey: .process)
            self = .process(
                pid: try inner.decode(Int32.self, forKey: .pid),
                startUs: try inner.decode(UInt64.self, forKey: .startUs)
            )
        } else {
            let inner = try outer.nestedContainer(keyedBy: ContainerKey.self, forKey: .container)
            self = .container(id: try inner.decode(String.self, forKey: .id))
        }
    }
}

public struct StopResult: Decodable, Equatable, Sendable {
    /// A request that got no usable reply; the empty container id is never a listed target.
    public static func unanswered(_ reason: String) -> StopResult {
        StopResult(target: .container(id: ""), result: .failed, reason: reason)
    }

    var shownReason: String {
        reason.flatMap { $0.isEmpty ? nil : $0 } ?? result.fallbackReason
    }

    public let target: StopTarget
    public let result: StopOutcome
    public let reason: String?

    public init(target: StopTarget, result: StopOutcome, reason: String?) {
        self.target = target
        self.result = result
        self.reason = reason
    }
}

public struct KillReply: Decodable, Equatable, Sendable {
    public let results: [StopResult]
}

/// One Stop control's life: ask, confirm, wait for the reply. Nothing is sent before `confirm`.
public enum StopFlow: Equatable, Sendable {
    case idle
    case asking(count: Int)
    case running
    case failed([String])

    public mutating func press(count: Int) {
        if self == .running { return }
        self = .asking(count: count)
    }

    /// True when the caller should send the request now.
    @discardableResult
    public mutating func confirm() -> Bool {
        guard case .asking = self else { return false }
        self = .running
        return true
    }

    /// The requests to send now: only the Core's non-empty ones, once, and only if the count still matches.
    public mutating func confirm(sending requests: [String], count: Int) -> [String] {
        guard case .asking(let asked) = self else { return [] }
        let usable = requests.filter { $0.isEmpty == false }
        guard asked == count, usable.isEmpty == false else {
            self = .idle
            return []
        }
        self = .running
        return usable
    }

    public mutating func confirm(_ offer: StopOffer) -> [String] {
        confirm(sending: [offer.request], count: offer.count)
    }

    public mutating func confirm(_ offer: StopAllOffer) -> [String] {
        confirm(sending: offer.requests, count: offer.ports)
    }

    public mutating func dismiss() {
        if self == .running { return }
        self = .idle
    }

    public mutating func finish(_ results: [StopResult]) {
        guard self == .running else { return }
        guard results.isEmpty == false else {
            self = .failed(["Navori got no answer to the stop."])
            return
        }
        let reasons = results.filter { $0.result.clearsRow == false }.map(\.shownReason)
        self = reasons.isEmpty ? .idle : .failed(reasons)
    }
}

/// Targets a stop reply has settled, kept until a later snapshot no longer lists them.
public struct StopProgress: Equatable, Sendable {
    public enum Entry: Equatable, Sendable {
        case stopping
        case failed(String)
    }

    private var entries: [StopTarget: Entry] = [:]

    public init() {}

    public mutating func record(_ results: [StopResult]) {
        for result in results {
            entries[result.target] = result.result.clearsRow ? .stopping : .failed(result.shownReason)
        }
    }

    public mutating func prune(listed: Set<StopTarget>) {
        entries = entries.filter { listed.contains($0.key) }
    }

    public func entry(for target: StopTarget) -> Entry? { entries[target] }

    public func isStopping(among targets: [StopTarget]) -> Bool {
        targets.contains { entries[$0] == .stopping }
    }

    public func failures(among targets: [StopTarget]) -> [String] {
        targets.compactMap { target in
            if case .failed(let reason) = entries[target] { reason } else { nil }
        }
    }
}
