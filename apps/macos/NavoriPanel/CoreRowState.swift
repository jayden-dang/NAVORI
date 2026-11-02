public enum StatusDot: Equatable, Sendable {
    case green, yellow, orange, red, gray
}

public enum CoreStatus: Sendable {
    case starting, running, needsAttention, notRunning
}

public struct CoreRowState: Equatable, Sendable {
    public let label: String
    public let dot: StatusDot?
    public let hasSubmenu: Bool

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
