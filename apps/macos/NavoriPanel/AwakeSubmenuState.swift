public enum AwakeBlockStatus: Equatable, Sendable {
    case off
    case on(since: String)
    case notHeld
    case unavailable
}

public struct AwakeLeaseInput: Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        case duration
        case untilOff
        case display
        case command
        case session
    }

    public let kind: Kind
    public let harness: String?
    public let label: String?
    public let pid: Int32?
    public let stopped: Bool
    public let until: String?

    public init(
        kind: Kind,
        harness: String?,
        label: String?,
        pid: Int32?,
        stopped: Bool,
        until: String?
    ) {
        self.kind = kind
        self.harness = harness
        self.label = label
        self.pid = pid
        self.stopped = stopped
        self.until = until
    }
}

public struct HoldingRow: Equatable, Sendable {
    public let line1: String
    public let line2: String
    public let accessibilitySentence: String

    public init(line1: String, line2: String, accessibilitySentence: String) {
        self.line1 = line1
        self.line2 = line2
        self.accessibilitySentence = accessibilitySentence
    }
}

public struct AwakeSubmenuState: Equatable, Sendable {
    public static let menuHoldSeconds = 3600

    public let statusTitle: String
    public let statusDetail: String
    public let releaseAllEnabled: Bool
    public let displayLabel: String
    public let holdingRows: [HoldingRow]

    public init(
        status: AwakeBlockStatus,
        leases: [AwakeLeaseInput],
        displayHeld: Bool,
        displayWanted: Bool
    ) {
        let counted = Self.leaseDetail(leases.count)
        switch status {
        case .off:
            statusTitle = "Awake off"
            statusDetail = "No Leases"
        case .on(let since):
            if displayWanted, displayHeld == false {
                statusTitle = "Display not held"
                statusDetail = counted
            } else {
                statusTitle = since.isEmpty ? "Awake" : "Awake · since \(since)"
                statusDetail = displayWanted && displayHeld ? "\(counted) · display kept on" : counted
            }
        case .notHeld:
            statusTitle = "Can't hold this Mac awake"
            statusDetail = "macOS refused the request. Navori keeps trying."
        case .unavailable:
            statusTitle = "Unavailable"
            statusDetail = ""
        }
        releaseAllEnabled = leases.isEmpty == false
        displayLabel = displayWanted ? "Display off" : "Display on"
        holdingRows = leases.map(Self.holdingRow)
    }

    private static func leaseDetail(_ count: Int) -> String {
        let noun = count == 1 ? "Lease" : "Leases"
        return "\(count) \(noun) · idle sleep blocked"
    }

    private static func holdingRow(_ lease: AwakeLeaseInput) -> HoldingRow {
        let line1: String
        let line2: String
        switch lease.kind {
        case .duration:
            line1 = text(lease.label) ?? "Manual"
            line2 = lease.until.map { "until \($0)" } ?? ""
        case .untilOff:
            line1 = text(lease.label) ?? "Until off"
            line2 = ""
        case .display:
            line1 = text(lease.label) ?? "Display"
            line2 = ""
        case .command:
            line1 = text(lease.label) ?? "command"
            line2 = pidLine("running", lease.pid)
        case .session:
            line1 = sessionLine(lease)
            let lead = lease.stopped ? "stopped · background work" : "working"
            line2 = pidLine(lead, lease.pid)
        }
        let spoken = line2.isEmpty ? line1 : "\(line1), \(line2)"
        return HoldingRow(line1: line1, line2: line2, accessibilitySentence: spoken)
    }

    private static func sessionLine(_ lease: AwakeLeaseInput) -> String {
        switch (text(lease.harness), text(lease.label)) {
        case let (harness?, place?):
            return "\(harness) · \(place)"
        case let (harness?, nil):
            return harness
        case let (nil, place?):
            return place
        case (nil, nil):
            return "Session"
        }
    }

    private static func pidLine(_ lead: String, _ pid: Int32?) -> String {
        guard let pid else { return lead }
        return "\(lead) · pid \(pid)"
    }

    private static func text(_ value: String?) -> String? {
        guard let value, value.isEmpty == false else { return nil }
        return value
    }
}
