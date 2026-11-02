public enum AwakeStatus: Equatable, Sendable {
    case off
    case on(sources: [String])
    case notHeld
    case unavailable
}

public struct AwakeRowState: Equatable, Sendable {
    public let label: String
    public let dot: StatusDot?
    public let hasSubmenu: Bool

    public init(status: AwakeStatus) {
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
        }
    }
}
