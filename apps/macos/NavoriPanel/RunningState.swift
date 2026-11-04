public enum RunningProjectState: Equatable, Sendable {
    case working, idle, unknown
}

public struct RunningSessionInput: Equatable, Sendable {
    public let stopped: Bool
    public let harness: String?
    public let title: String?
    public let branch: String?

    public init(stopped: Bool, harness: String? = nil, title: String? = nil, branch: String? = nil) {
        self.stopped = stopped
        self.harness = harness
        self.title = title
        self.branch = branch
    }
}

public struct RunningPortInput: Equatable, Sendable {
    public let port: Int
    public let name: String
    /// Empty when the Core offers no Stop for this port.
    public let stopRequest: String
    public let target: StopTarget?
    public let kind: String
    public let worktree: String?
    public let harness: String?
    public let container: Bool
    public let protectedInGroup: Bool

    public init(
        port: Int,
        name: String,
        stopRequest: String,
        target: StopTarget? = nil,
        kind: String = "other",
        worktree: String? = nil,
        harness: String? = nil,
        container: Bool = false,
        protectedInGroup: Bool = false
    ) {
        self.port = port
        self.name = name
        self.stopRequest = stopRequest
        self.target = target
        self.kind = kind
        self.worktree = worktree
        self.harness = harness
        self.container = container
        self.protectedInGroup = protectedInGroup
    }

    public var canStop: Bool { stopRequest.isEmpty == false }
}

public struct RunningProjectInput: Equatable, Sendable {
    public let name: String
    public let isOther: Bool
    public let state: RunningProjectState
    public let sessions: [RunningSessionInput]
    public let ports: [RunningPortInput]
    public let stoppablePorts: Int
    public let rowStopRequest: String?
    public let childStopRequest: String?
    public let path: String?
    public let upSinceMs: Int64?

    public init(
        name: String,
        isOther: Bool,
        state: RunningProjectState,
        sessions: [RunningSessionInput],
        ports: [RunningPortInput],
        stoppablePorts: Int,
        rowStopRequest: String?,
        childStopRequest: String?,
        path: String? = nil,
        upSinceMs: Int64? = nil
    ) {
        self.path = path
        self.upSinceMs = upSinceMs
        self.name = name
        self.isOther = isOther
        self.state = state
        self.sessions = sessions
        self.ports = ports
        self.stoppablePorts = stoppablePorts
        self.rowStopRequest = rowStopRequest
        self.childStopRequest = childStopRequest
    }
}

public struct RunningInput: Equatable, Sendable {
    public let unknownReason: String?
    public let projects: [RunningProjectInput]
    public let hiddenSystemPorts: Int
    public let stopAll: [String]
    public let stopAllPorts: Int
    public let inUsePorts: Int
    public let otherPortsLeft: Int

    public init(
        unknownReason: String?,
        projects: [RunningProjectInput],
        hiddenSystemPorts: Int,
        stopAll: [String],
        stopAllPorts: Int,
        inUsePorts: Int,
        otherPortsLeft: Int
    ) {
        self.unknownReason = unknownReason
        self.projects = projects
        self.hiddenSystemPorts = hiddenSystemPorts
        self.stopAll = stopAll
        self.stopAllPorts = stopAllPorts
        self.inUsePorts = inUsePorts
        self.otherPortsLeft = otherPortsLeft
    }

    var isActive: Bool {
        projects.contains { project in
            project.ports.isEmpty == false || project.sessions.contains { $0.stopped == false }
        }
    }
}

/// Level-0 `Running` row. The row draws no text; `spoken` is what VoiceOver reads.
public struct RunningRowState: Equatable, Sendable {
    public let label = "Running"
    public let dot: StatusDot?
    public let hasSubmenu: Bool
    public let spoken: String

    public init(running: RunningInput?) {
        let active = running?.isActive ?? false
        dot = active ? .green : nil
        hasSubmenu = running != nil
        spoken = active ? "Running, active" : "Running, nothing running"
    }
}

public enum ProjectDot: Equatable, Sendable {
    case filled, ring, none
}

public struct StopOffer: Equatable, Sendable {
    public let count: Int
    public let request: String

    public init(count: Int, request: String) {
        self.count = count
        self.request = request
    }

    public var spoken: String { "Stop \(count) \(count == 1 ? "port" : "ports")" }

    public var confirmation: String { "Stop \(count) \(count == 1 ? "port" : "ports")?" }

    init?(count: Int, request: String?) {
        guard let request, request.isEmpty == false else { return nil }
        self.init(count: count, request: request)
    }
}

public struct ProjectRowState: Equatable, Sendable {
    public let id: String
    public let name: String
    public let dot: ProjectDot
    public let spoken: String
    /// `Stop N` on the row. Present only when the Core sent a row request.
    public let stop: StopOffer?
    /// `Stop N ports` in the child menu. Present only when the Core sent a child request.
    public let childStop: StopOffer?
    public let targets: [StopTarget]
    public let childRowIds: [String]
    public let childOffers: [String: StopOffer]

    init(_ project: RunningProjectInput) {
        targets = project.ports.compactMap(\.target)
        let id = Self.id(of: project)
        self.id = id
        name = project.name
        stop = StopOffer(count: project.stoppablePorts, request: project.rowStopRequest)
        childStop = StopOffer(count: project.stoppablePorts, request: project.childStopRequest)
        var ids = project.ports.map { ProjectChildState.portId(project: id, port: $0.port) }
        var offers: [String: StopOffer] = [:]
        for port in project.ports {
            if port.canStop {
                offers[ProjectChildState.portId(project: id, port: port.port)] = StopOffer(count: 1, request: port.stopRequest)
            }
        }
        if let childStop {
            ids.append(ProjectChildState.stopId(project: id))
            offers[ProjectChildState.stopId(project: id)] = childStop
        }
        childRowIds = ids
        childOffers = offers
        switch project.state {
        case .working:
            dot = .filled
            spoken = "\(project.name), working"
        case .idle:
            dot = .ring
            let ports = project.ports.count
            spoken = ports == 0
                ? "\(project.name), idle"
                : "\(project.name), \(ports) idle \(ports == 1 ? "port" : "ports")"
        case .unknown:
            dot = .none
            spoken = "\(project.name), working state unknown"
        }
    }
}

extension ProjectRowState {
    public static func id(of project: RunningProjectInput) -> String {
        project.isOther ? "running.other" : "running.project.\(project.name)"
    }
}

public struct StopAllOffer: Equatable, Sendable {
    public let requests: [String]
    public let ports: Int
    public let projects: Int
    public let inUse: Int
    public let otherLeft: Int
    public let systemHidden: Int

    public init(requests: [String], ports: Int, projects: Int, inUse: Int, otherLeft: Int, systemHidden: Int) {
        self.requests = requests
        self.ports = ports
        self.projects = projects
        self.inUse = inUse
        self.otherLeft = otherLeft
        self.systemHidden = systemHidden
    }
}

extension StopAllOffer {
    public var spoken: String { "Stop \(ports) \(ports == 1 ? "port" : "ports")" }

    public var confirmation: String {
        var text = "Stop \(ports) \(ports == 1 ? "port" : "ports") in \(projects) \(projects == 1 ? "project" : "projects")?"
        var left: [String] = []
        if inUse > 0 { left.append("\(inUse) \(inUse == 1 ? "port" : "ports") in use") }
        if otherLeft > 0 { left.append("\(otherLeft) \(otherLeft == 1 ? "port" : "ports") in Other") }
        if left.isEmpty == false { text += " \(left.joined(separator: " and ")) stay running." }
        return text
    }

    public var systemNote: String? {
        systemHidden > 0 ? "\(systemHidden) system \(systemHidden == 1 ? "port" : "ports") hidden" : nil
    }
}

public enum RunningSubmenuItem: Equatable, Sendable {
    case keepAwake
    case trackingAgents
    case note(String)
    case problem(String)
    case separator
    case nothingRunning
    case project(ProjectRowState)
    case commands([HoldingRow])
    case stopAll(StopAllOffer)
}

public struct RunningSubmenuState: Equatable, Sendable {
    public let rows: [ProjectRowState]
    public let commands: [HoldingRow]
    public let stopAll: StopAllOffer?
    public let nothingRunning: Bool
    /// The one line in the problem line's place: why Stop is off, else the Awake problem.
    public let note: String?
    private let noteIsProblem: Bool

    /// `problem` is the Awake problem text, shown only when the Core gives no reason of its own.
    public init(running: RunningInput, problem: String? = nil, commands: [HoldingRow] = []) {
        self.commands = commands
        nothingRunning = running.isActive == false
        rows = nothingRunning ? [] : running.projects.map(ProjectRowState.init)
        let reason = running.unknownReason.flatMap { $0.isEmpty ? nil : $0 }
        noteIsProblem = reason == nil
        note = reason ?? problem.flatMap { $0.isEmpty ? nil : $0 }
        stopAll = running.stopAll.isEmpty ? nil : StopAllOffer(
            requests: running.stopAll,
            ports: running.stopAllPorts,
            projects: running.stopAll.count,
            inUse: running.inUsePorts,
            otherLeft: running.otherPortsLeft,
            systemHidden: running.hiddenSystemPorts
        )
    }

    public var items: [RunningSubmenuItem] {
        var items: [RunningSubmenuItem] = [.keepAwake, .trackingAgents]
        if let note { items.append(noteIsProblem ? .problem(note) : .note(note)) }
        items.append(.separator)
        items += nothingRunning ? [.nothingRunning] : rows.map(RunningSubmenuItem.project)
        if commands.isEmpty == false { items += [.separator, .commands(commands)] }
        if let stopAll { items += [.separator, .stopAll(stopAll)] }
        return items
    }
}

public enum ProjectChildSection: Equatable, Sendable {
    case sessions, ports
}

public struct ProjectChildHeader: Equatable, Sendable {
    public let name: String
    public let path: String?
    public let up: String?
}

public enum PortMark: Equatable, Sendable {
    case container, process, database, system
}

public struct SessionRowState: Equatable, Sendable {
    public let mark: HoldingMark
    public let title: String
    public let detail: String
    public let dimmed: Bool
    public let showsDot: Bool
    public let spoken: String

    init(_ session: RunningSessionInput) {
        let title = Self.text(session.title) ?? Self.text(session.harness) ?? "Session"
        let word = session.stopped ? "stopped" : "working"
        let branch = Self.text(session.branch)
        self.title = title
        mark = AwakeSubmenuState.harnessMark(session.harness)
        detail = branch.map { "\($0) \u{00B7} \(word)" } ?? word
        dimmed = session.stopped
        showsDot = session.stopped == false
        var parts = [title]
        if let harness = Self.text(session.harness), harness != title { parts.append(harness) }
        if let branch { parts.append("branch \(branch)") }
        parts.append(word)
        spoken = parts.joined(separator: ", ")
    }

    private static func text(_ value: String?) -> String? {
        value.flatMap { $0.isEmpty ? nil : $0 }
    }
}

public struct PortRowState: Equatable, Sendable {
    public let id: String
    public let mark: PortMark
    public let name: String
    public let suffix: String?
    public let portText: String
    public let tooltip: String?
    public let spoken: String
    public let stop: StopOffer?
    public let target: StopTarget?
    private let sentence: String

    init(_ port: RunningPortInput, projectId: String) {
        id = ProjectChildState.portId(project: projectId, port: port.port)
        name = port.name
        portText = ":\(port.port)"
        suffix = port.container ? "docker" : nil
        stop = port.canStop ? StopOffer(count: 1, request: port.stopRequest) : nil
        target = port.target
        if port.container {
            mark = .container
        } else {
            switch port.kind {
            case "database": mark = .database
            case "system": mark = .system
            default: mark = .process
            }
        }
        var origin: [String] = []
        if let worktree = port.worktree, worktree.isEmpty == false { origin.append("worktree \(worktree)") }
        if port.container == false, let harness = port.harness.flatMap(Self.harnessName) {
            origin.append("started by \(harness)")
        }
        tooltip = origin.isEmpty ? nil : origin.joined(separator: " \u{00B7} ")
        var parts = [port.name]
        if port.container { parts.append("docker") }
        parts.append("port \(port.port)")
        sentence = (parts + origin).joined(separator: ", ")
        spoken = "\(sentence), listening"
    }

    public func spoken(for entry: StopProgress.Entry?) -> String {
        switch entry {
        case .none: spoken
        case .stopping: "\(sentence), stopping"
        case let .failed(reason): "\(sentence), stop failed, \(reason)"
        }
    }

    private static func harnessName(_ harness: String) -> String? {
        switch harness {
        case "claude_code": "Claude Code"
        case "codex": "Codex"
        case "grok": "Grok"
        case "": nil
        default: harness
        }
    }
}

public struct ProjectChildState: Equatable, Sendable {
    public let id: String
    public let header: ProjectChildHeader
    public let sessions: [SessionRowState]
    public let ports: [PortRowState]
    public let stop: StopOffer?
    public let stopId: String
    public let systemNote: String?

    public init(project: RunningProjectInput, nowMs: Int64, home: String) {
        let id = ProjectRowState.id(of: project)
        self.id = id
        stopId = Self.stopId(project: id)
        header = ProjectChildHeader(
            name: project.name,
            path: project.path.flatMap { $0.isEmpty ? nil : Self.abbreviated($0, home: home) },
            up: project.state == .idle ? project.upSinceMs.map { Self.up(nowMs - $0) } : nil
        )
        sessions = project.sessions.map(SessionRowState.init)
        ports = project.ports.map { PortRowState($0, projectId: id) }
        stop = StopOffer(count: project.stoppablePorts, request: project.childStopRequest)
        let hidden = project.ports.filter(\.protectedInGroup).count
        systemNote = project.isOther && hidden > 0
            ? "\(hidden) system \(hidden == 1 ? "port" : "ports") hidden"
            : nil
    }

    public var sections: [ProjectChildSection] {
        (sessions.isEmpty ? [] : [.sessions]) + (ports.isEmpty ? [] : [.ports])
    }

    public static func portId(project: String, port: Int) -> String { "\(project).port.\(port)" }

    public static func stopId(project: String) -> String { "\(project).stop" }

    private static func abbreviated(_ path: String, home: String) -> String {
        guard home.isEmpty == false, home != "/" else { return path }
        if path == home { return "~" }
        return path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
    }

    private static func up(_ elapsedMs: Int64) -> String {
        let minutes = max(0, elapsedMs) / 60_000
        let days = minutes / 1440
        let hours = minutes % 1440 / 60
        if days > 0 { return "up \(days)d \(hours)h" }
        if hours > 0 { return "up \(hours)h \(minutes % 60)m" }
        return minutes > 0 ? "up \(minutes)m" : "up <1m"
    }
}
