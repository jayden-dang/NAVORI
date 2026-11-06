/// One row of the `Keep awake` Submenu.
public struct KeepAwakeChoice: Equatable, Sendable {
    /// Nil is `Indefinitely`.
    public let seconds: Int?
    /// For a length, this is also the label the Awake Module publishes for that duration Lease.
    public let title: String

    public static let indefinitely = KeepAwakeChoice(seconds: nil, title: "Indefinitely")

    public static let all: [KeepAwakeChoice] = [
        indefinitely,
        KeepAwakeChoice(seconds: 300, title: "5 minutes"),
        KeepAwakeChoice(seconds: 600, title: "10 minutes"),
        KeepAwakeChoice(seconds: 900, title: "15 minutes"),
        KeepAwakeChoice(seconds: 1800, title: "30 minutes"),
        KeepAwakeChoice(seconds: 3600, title: "1 hour"),
        KeepAwakeChoice(seconds: 7200, title: "2 hours"),
        KeepAwakeChoice(seconds: 18000, title: "5 hours"),
    ]

    private init(seconds: Int?, title: String) {
        self.seconds = seconds
        self.title = title
    }

    public init?(id: String) {
        guard let found = Self.all.first(where: { $0.id == id }) else { return nil }
        self = found
    }

    /// The cascade row id.
    public var id: String {
        "awake.keep.\(seconds.map(String.init) ?? "indefinitely")"
    }
}

/// What the Awake Submenu can ask the Awake Module to do.
public enum AwakeCommand: Equatable, Sendable {
    case keepFor(seconds: Int)
    case keepIndefinitely
    case keepOff
    case tracking(Bool)
    case lid(Bool)

    public var actionJSON: String {
        switch self {
        case .keepFor(let seconds): "{\"keep_for\":{\"seconds\":\(seconds)}}"
        case .keepIndefinitely: "\"keep_indefinitely\""
        case .keepOff: "\"keep_off\""
        case .tracking(let on): "{\"tracking\":{\"on\":\(on)}}"
        case .lid(let on): "{\"lid\":{\"on\":\(on)}}"
        }
    }
}
