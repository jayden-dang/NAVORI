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

/// Decides which motion the status item plays. It remembers the last state it saw, so a Beat
/// fires only when attention newly turns on after the icon has drawn a real snapshot.
public struct StatusIconMotion: Sendable {
    public struct Plan: Equatable, Sendable {
        /// Fade the icon up and down for as long as the glyph is AI running.
        public let loop: Bool
        /// Dip the icon once, because attention just turned on.
        public let beat: Bool

        public init(loop: Bool, beat: Bool) {
            self.loop = loop
            self.beat = beat
        }
    }

    private var previous: StatusIconState?

    public init() {}

    public mutating func next(_ state: StatusIconState, reduceMotion: Bool, allowBeat: Bool) -> Plan {
        defer { previous = state }
        guard !reduceMotion else { return Plan(loop: false, beat: false) }
        let loop = state.glyph == .aiRunning
        guard allowBeat, state.attention, let previous,
              !previous.attention, previous.glyph != .starting, previous.glyph != .coreDown
        else { return Plan(loop: loop, beat: false) }
        return Plan(loop: loop, beat: true)
    }
}
