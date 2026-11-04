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

public enum StopTarget: Decodable, Equatable, Sendable {
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
        let reasons = results.filter { $0.result.clearsRow == false }.map { result in
            result.reason.flatMap { $0.isEmpty ? nil : $0 } ?? result.result.fallbackReason
        }
        self = reasons.isEmpty ? .idle : .failed(reasons)
    }
}
