public enum StatusDot: Equatable, Sendable {
    case green, yellow, orange, red, gray
}

public enum CoreHealth: Sendable {
    case starting, ok, degraded, down
}

extension CoreHealth {
    /// Only `failed` and `restarting` Modules, or a config notice, make the Core degraded.
    public init(coreFailed: Bool, hasSnapshot: Bool, moduleStates: [String], hasConfigNotice: Bool) {
        if coreFailed {
            self = .down
        } else if !hasSnapshot {
            self = .starting
        } else if hasConfigNotice || moduleStates.contains(where: { $0 == "failed" || $0 == "restarting" }) {
            self = .degraded
        } else {
            self = .ok
        }
    }
}

public enum CoreStatus: Equatable, Sendable {
    case starting, running, needsAttention, notRunning

    public init(health: CoreHealth) {
        switch health {
        case .starting: self = .starting
        case .ok: self = .running
        case .degraded: self = .needsAttention
        case .down: self = .notRunning
        }
    }
}

public struct CoreRowState: Equatable, Sendable {
    public let label: String
    public let dot: StatusDot?
    public let hasSubmenu: Bool

    /// The dev profile always shows the Core row; production shows it only when the Core needs attention or is down.
    public static func isVisible(status: CoreStatus, inDevProfile: Bool) -> Bool {
        if inDevProfile { return true }
        switch status {
        case .needsAttention, .notRunning: return true
        case .starting, .running: return false
        }
    }

    public init(status: CoreStatus) {
        switch status {
        case .starting:
            label = "Starting"
            dot = nil
            hasSubmenu = false
        case .running:
            label = "Running"
            dot = .green
            hasSubmenu = true
        case .needsAttention:
            label = "Needs attention"
            dot = .orange
            hasSubmenu = true
        case .notRunning:
            label = "Not running"
            dot = .red
            hasSubmenu = false
        }
    }
}

public enum ModuleDot {
    public static func color(state: String) -> StatusDot {
        switch state {
        case "running": .green
        case "starting", "restarting": .yellow
        case "failed": .red
        default: .gray
        }
    }
}
