public enum AwakeStatus: Equatable, Sendable {
    case off
    case on(sources: [String])
    case notHeld
    case unavailable
    /// A problem other than a stopped module or a refused assertion. `held` says a Lease has the system assertion.
    case problem(String, held: Bool)
}

/// Level-0 Awake row from the snapshot's problem text and whether the system assertion is held.
public enum AwakeLevel0 {
    public static let moduleStopped = "The Awake module stopped."

    public static func row(
        problem: String?,
        systemHeld: Bool,
        hasLease: Bool,
        sources: [String]
    ) -> AwakeStatus {
        if problem == moduleStopped {
            return .unavailable
        }
        if hasLease, systemHeld == false {
            return .notHeld
        }
        if let problem, problem.isEmpty == false {
            return .problem(problem, held: hasLease)
        }
        if hasLease == false {
            return .off
        }
        return .on(sources: sources)
    }
}

/// A menu action that threw. The message stays until a later snapshot achieves the request.
public struct AwakeActionFailure: Equatable, Sendable {
    public let request: AwakeCommand
    public let message: String
    private let marks: AwakeHeldMarks

    public init(request: AwakeCommand, message: String, marks: AwakeHeldMarks) {
        self.request = request
        self.message = message
        self.marks = marks
    }

    /// The message to show, or nil once a later snapshot achieved this request.
    public func visible(against later: AwakeHeldMarks) -> String? {
        achieved(later) ? nil : message
    }

    private func achieved(_ later: AwakeHeldMarks) -> Bool {
        switch request {
        case .keepFor:
            guard let laterEnd = later.durationEndsAtMs else { return false }
            return laterEnd != marks.durationEndsAtMs
        case .keepIndefinitely:
            return later.hasDisplay && marks.hasDisplay == false
        case .keepOff:
            return later.durationEndsAtMs == nil && later.hasUntilOff == false && later.hasDisplay == false
        case .tracking(let on):
            return later.tracking == on
        }
    }
}

public struct AwakeHeldMarks: Equatable, Sendable {
    public var durationEndsAtMs: Int64?
    public var hasUntilOff: Bool
    public var hasDisplay: Bool
    public var tracking: Bool

    public init(durationEndsAtMs: Int64?, hasUntilOff: Bool, hasDisplay: Bool, tracking: Bool) {
        self.durationEndsAtMs = durationEndsAtMs
        self.hasUntilOff = hasUntilOff
        self.hasDisplay = hasDisplay
        self.tracking = tracking
    }
}

public struct AwakeRowState: Equatable, Sendable {
    public let label: String
    public let dot: StatusDot?
    public let hasSubmenu: Bool

    private static let needsAttention = "Needs attention"

    /// `dot` is green only while a Lease has the system assertion. `label` is what VoiceOver reads; the row draws no text.
    public init(status: AwakeStatus, actionFailure: String? = nil) {
        var text: String
        var submenu = true
        switch status {
        case .off:
            text = "Off"
            dot = nil
        case .on(let sources):
            let names = Self.counted(sources)
            text = names.isEmpty ? "On" : "On · \(names.joined(separator: ", "))"
            dot = .green
        case .notHeld:
            text = "Not held"
            dot = nil
        case .unavailable:
            text = "Unavailable"
            dot = nil
            submenu = false
        case .problem(_, let held):
            text = Self.needsAttention
            dot = held ? .green : nil
        }
        if let actionFailure, actionFailure.isEmpty == false {
            text = Self.needsAttention
            submenu = true
        }
        label = text
        hasSubmenu = submenu
    }

    /// Each source once, in first-seen order, with a count when it holds more than one Lease.
    private static func counted(_ sources: [String]) -> [String] {
        var order: [String] = []
        var counts: [String: Int] = [:]
        for source in sources where source.isEmpty == false {
            if counts[source] == nil { order.append(source) }
            counts[source, default: 0] += 1
        }
        return order.map { name in
            let count = counts[name, default: 1]
            return count > 1 ? "\(name) ×\(count)" : name
        }
    }
}
