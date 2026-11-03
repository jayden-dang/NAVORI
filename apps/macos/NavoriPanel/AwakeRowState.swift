public enum AwakeStatus: Equatable, Sendable {
    case off
    case on(sources: [String])
    case notHeld
    case unavailable
    /// A problem other than a stopped module, while the system assertion is held.
    case problem(String)
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
            return .problem(problem)
        }
        if hasLease == false {
            return .off
        }
        return .on(sources: sources)
    }
}

/// A menu action that threw. The message stays until a later snapshot achieves the request.
public struct AwakeActionFailure: Equatable, Sendable {
    public enum Request: Equatable, Sendable {
        case duration
        case untilOff
        case display(Bool)
        case releaseAll
    }

    public let request: Request
    public let message: String
    private let marks: AwakeHeldMarks

    public init(request: Request, message: String, marks: AwakeHeldMarks) {
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
        case .duration:
            guard let laterEnd = later.durationEndsAtMs else { return false }
            return laterEnd != marks.durationEndsAtMs
        case .untilOff:
            return later.hasUntilOff && marks.hasUntilOff == false
        case .display(true):
            return later.hasDisplay && marks.hasDisplay == false
        case .display(false):
            return marks.hasDisplay && later.hasDisplay == false
        case .releaseAll:
            return marks.hasLease && later.hasLease == false
        }
    }
}

public struct AwakeHeldMarks: Equatable, Sendable {
    public var durationEndsAtMs: Int64?
    public var hasUntilOff: Bool
    public var hasDisplay: Bool
    public var hasLease: Bool

    public init(durationEndsAtMs: Int64?, hasUntilOff: Bool, hasDisplay: Bool, hasLease: Bool) {
        self.durationEndsAtMs = durationEndsAtMs
        self.hasUntilOff = hasUntilOff
        self.hasDisplay = hasDisplay
        self.hasLease = hasLease
    }
}

public struct AwakeRowState: Equatable, Sendable {
    public let label: String
    public let dot: StatusDot?
    public let hasSubmenu: Bool

    public init(status: AwakeStatus, actionFailure: String? = nil) {
        if let actionFailure, actionFailure.isEmpty == false {
            label = actionFailure
            dot = .orange
            hasSubmenu = true
            return
        }
        switch status {
        case .off:
            label = "Off"
            dot = nil
            hasSubmenu = true
        case .on(let sources):
            let names = sources.filter { $0.isEmpty == false }
            label = names.isEmpty ? "On" : "On · \(names.joined(separator: ", "))"
            dot = .green
            hasSubmenu = true
        case .notHeld:
            label = "Not held"
            dot = .orange
            hasSubmenu = true
        case .unavailable:
            label = "Unavailable"
            dot = nil
            hasSubmenu = false
        case .problem(let text):
            label = text
            dot = .orange
            hasSubmenu = true
        }
    }
}
