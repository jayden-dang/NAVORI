/// Which mark the status item shows. The first four come from the Core; the last two are the Shell's
/// own lifecycle readings of "the Core cannot speak".
public enum StatusIconGlyph: Equatable, Sendable {
    case normal, lease, aiRunning, problem, coreDown, starting
}

public struct StatusIconState: Equatable, Sendable {
    public let glyph: StatusIconGlyph
    /// Something waits for the user.
    public let attention: Bool
    /// One line for the status item's accessibility description.
    public let description: String

    public init(glyph: StatusIconGlyph, attention: Bool, description: String) {
        self.glyph = glyph
        self.attention = attention
        self.description = description
    }
}

extension StatusIconState {
    /// A failed Core wins over anything it said; with nothing said yet the Core is starting.
    public init(coreFailed: Bool, core: StatusIconState?) {
        if coreFailed {
            self.init(glyph: .coreDown, attention: false, description: "Core not running")
        } else if let core {
            self = core
        } else {
            self.init(glyph: .starting, attention: false, description: "Starting")
        }
    }
}
