import Foundation

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

public enum HoldingMark: Equatable, Sendable {
    case claude
    case codex
    case grok
    case terminal
    case clock
    case infinity
    case display
}

public struct HoldingRow: Equatable, Sendable {
    public let mark: HoldingMark
    public let name: String
    public let word: String
    public let showsDot: Bool
    public let isQuiet: Bool
    public let help: String?
    public let accessibilitySentence: String
}

public struct HoldingGroup: Equatable, Sendable {
    public let title: String
    public let rows: [HoldingRow]
}

/// The only status text the Submenu shows: something is wrong.
public struct AwakeProblem: Equatable, Sendable {
    public let title: String
    public let detail: String?

    public init(title: String, detail: String?) {
        self.title = title
        self.detail = detail
    }
}

public struct AwakeSubmenuState: Equatable, Sendable {
    public let problem: AwakeProblem?
    /// `until <time>`, `Indefinitely`, or nil while no Keep awake lease exists.
    public let keepValue: String?
    public let trackingOn: Bool
    public let holdingGroups: [HoldingGroup]
    private let checkedTitles: [String]

    public init(
        status: AwakeStatus,
        leases: [AwakeLeaseInput],
        displayHeld: Bool,
        tracking: Bool,
        actionFailure: String? = nil
    ) {
        let duration = leases.first { $0.kind == .duration }
        let hasDisplay = leases.contains { $0.kind == .display }
        let indefinite = hasDisplay || leases.contains { $0.kind == .untilOff }
        let base = Self.statusProblem(
            status,
            displayRefused: hasDisplay && displayHeld == false,
            leaseCount: leases.count
        )
        problem = Self.withFailure(base, actionFailure)
        keepValue = duration?.until.map { "until \($0)" } ?? (indefinite ? KeepAwakeChoice.indefinitely.title : nil)
        trackingOn = tracking
        checkedTitles = [duration?.label, indefinite ? KeepAwakeChoice.indefinitely.title : nil].compactMap { $0 }
        holdingGroups = Self.groups(leases, systemHeld: status != .notHeld)
    }

    public var trackingValue: String { trackingOn ? "On" : "Off" }
    public var trackingDot: StatusDot? { trackingOn ? .green : nil }

    public func isChecked(_ choice: KeepAwakeChoice) -> Bool {
        checkedTitles.contains(choice.title)
    }

    /// Picking the checked choice turns Keep awake off. Any other choice replaces it.
    public func command(picking choice: KeepAwakeChoice) -> AwakeCommand {
        if isChecked(choice) { return .keepOff }
        guard let seconds = choice.seconds else { return .keepIndefinitely }
        return .keepFor(seconds: seconds)
    }

    private static func statusProblem(
        _ status: AwakeStatus,
        displayRefused: Bool,
        leaseCount: Int
    ) -> AwakeProblem? {
        switch status {
        case .notHeld:
            return AwakeProblem(
                title: "Can't hold this Mac awake",
                detail: "macOS refused the request. Navori keeps trying."
            )
        case .problem(let text, _):
            return AwakeProblem(title: text, detail: nil)
        case .unavailable:
            return AwakeProblem(title: "Unavailable", detail: nil)
        case .on:
            guard displayRefused else { return nil }
            let noun = leaseCount == 1 ? "Lease" : "Leases"
            return AwakeProblem(title: "Display not held", detail: "\(leaseCount) \(noun) · idle sleep blocked")
        case .off:
            return nil
        }
    }

    private static func withFailure(_ base: AwakeProblem?, _ failure: String?) -> AwakeProblem? {
        guard let failure, failure.isEmpty == false else { return base }
        guard let base else { return AwakeProblem(title: failure, detail: nil) }
        return AwakeProblem(title: base.title, detail: failure)
    }

    /// Held by you, then Commands, then projects by name, so a group does not move when a session's state changes.
    private static func groups(_ leases: [AwakeLeaseInput], systemHeld: Bool) -> [HoldingGroup] {
        let manual = [AwakeLeaseInput.Kind.duration, .untilOff, .display].flatMap { kind in
            leases.filter { $0.kind == kind }.map { manualRow($0, systemHeld: systemHeld) }
        }
        let commands = leases.filter { $0.kind == .command }
            .sorted { ($0.pid ?? 0) < ($1.pid ?? 0) }
            .map(commandRow)
        var projects: [String: [AwakeLeaseInput]] = [:]
        for lease in leases where lease.kind == .session {
            projects[place(lease).project, default: []].append(lease)
        }
        let sessions = projects.keys
            .sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
            .map { project in
                HoldingGroup(
                    title: project,
                    rows: projects[project, default: []].sorted(by: sessionOrder).map(sessionRow)
                )
            }
        var groups: [HoldingGroup] = []
        if manual.isEmpty == false { groups.append(HoldingGroup(title: "Held by you", rows: manual)) }
        if commands.isEmpty == false { groups.append(HoldingGroup(title: "Commands", rows: commands)) }
        return groups + sessions
    }

    /// Working before stopped, then by name, then by pid.
    private static func sessionOrder(_ left: AwakeLeaseInput, _ right: AwakeLeaseInput) -> Bool {
        if left.stopped != right.stopped { return right.stopped }
        let names = place(left).name.localizedCaseInsensitiveCompare(place(right).name)
        if names != .orderedSame { return names == .orderedAscending }
        return (left.pid ?? 0) < (right.pid ?? 0)
    }

    /// `project/branch`. The branch keeps any further slashes.
    private static func place(_ lease: AwakeLeaseInput) -> (project: String, name: String) {
        guard let label = text(lease.label) else {
            return ("Other", text(lease.harness) ?? "Session")
        }
        guard let slash = label.firstIndex(of: "/") else { return (label, label) }
        let project = String(label[..<slash])
        let name = String(label[label.index(after: slash)...])
        guard project.isEmpty == false, name.isEmpty == false else { return (label, label) }
        return (project, name)
    }

    private static func sessionRow(_ lease: AwakeLeaseInput) -> HoldingRow {
        let word = lease.stopped ? "stopped" : "working"
        let source = [text(lease.harness), text(lease.label)].compactMap { $0 }
        let pid = lease.pid.map { "pid \($0)" }
        return HoldingRow(
            mark: harnessMark(lease.harness),
            name: place(lease).name,
            word: word,
            showsDot: lease.stopped == false,
            isQuiet: lease.stopped,
            help: (source + [pid].compactMap { $0 }).joined(separator: " · "),
            accessibilitySentence: (source + [word, pid].compactMap { $0 }).joined(separator: ", ")
        )
    }

    private static func commandRow(_ lease: AwakeLeaseInput) -> HoldingRow {
        let name = text(lease.label) ?? "command"
        let pid = lease.pid.map { "pid \($0)" }
        return HoldingRow(
            mark: .terminal,
            name: name,
            word: "running",
            showsDot: false,
            isQuiet: false,
            help: pid,
            accessibilitySentence: ([name, "running", pid].compactMap { $0 }).joined(separator: ", ")
        )
    }

    private static func manualRow(_ lease: AwakeLeaseInput, systemHeld: Bool) -> HoldingRow {
        let name: String
        let mark: HoldingMark
        var word = ""
        switch lease.kind {
        case .duration:
            name = text(lease.label) ?? "Manual"
            mark = .clock
            word = lease.until.map { "until \($0)" } ?? ""
        case .display:
            name = text(lease.label) ?? "Display"
            mark = .display
        default:
            name = text(lease.label) ?? "Until off"
            mark = .infinity
        }
        return HoldingRow(
            mark: mark,
            name: name,
            word: word,
            showsDot: false,
            isQuiet: systemHeld == false,
            help: nil,
            accessibilitySentence: word.isEmpty ? name : "\(name), \(word)"
        )
    }

    private static func harnessMark(_ harness: String?) -> HoldingMark {
        switch harness {
        case "Claude Code": .claude
        case "Codex": .codex
        case "Grok": .grok
        default: .terminal
        }
    }

    private static func text(_ value: String?) -> String? {
        guard let value, value.isEmpty == false else { return nil }
        return value
    }
}
