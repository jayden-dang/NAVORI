import Foundation

private func plural(_ n: Int, _ noun: String) -> String { n == 1 ? noun : noun + "s" }

private func counted(_ n: Int, _ noun: String) -> String { "\(n) \(plural(n, noun))" }

private func systemPortsHidden(_ n: Int) -> String { "\(n) system \(plural(n, "port")) hidden" }

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
    public let openable: Bool
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
        openable: Bool = false,
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
        self.openable = openable
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

    public var spoken: String { "Stop \(counted(count, "port"))" }

    public var confirmation: String { "Stop \(counted(count, "port"))?" }

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
    /// `Stop N ports` in the child menu. Present only when the Core sent a child request.
    public let childStop: StopOffer?
    public let childRowIds: [String]
    public let childOffers: [String: StopOffer]
    /// One id per session row of the child menu. They are shown, never acted on.
    public let sessionRowIds: [String]

    init(_ project: RunningProjectInput) {
        let id = Self.id(of: project)
        self.id = id
        name = project.name
        childStop = StopOffer(count: project.stoppablePorts, request: project.childStopRequest)
        var ids = project.ports.map { ProjectChildState.portId(project: id, port: $0) }
        var offers: [String: StopOffer] = [:]
        for port in project.ports {
            if port.canStop {
                offers[ProjectChildState.portId(project: id, port: port)] = StopOffer(count: 1, request: port.stopRequest)
            }
        }
        if let childStop {
            ids.append(ProjectChildState.stopId(project: id))
            offers[ProjectChildState.stopId(project: id)] = childStop
        }
        childRowIds = ids
        childOffers = offers
        sessionRowIds = project.sessions.indices.map { ProjectChildState.sessionId(project: id, index: $0) }
        switch project.state {
        case .working:
            dot = .filled
            spoken = "\(project.name), working"
        case .idle:
            dot = .ring
            let ports = project.ports.count
            spoken = ports == 0
                ? "\(project.name), idle"
                : "\(project.name), \(ports) idle \(plural(ports, "port"))"
        case .unknown:
            dot = .none
            spoken = "\(project.name), working state unknown"
        }
    }
}

extension ProjectRowState {
    /// The row opens a child menu exactly when the child has something to show: a session or a port.
    /// The chevron and the cascade node both follow this one rule.
    public var hasChild: Bool { sessionRowIds.isEmpty == false || childRowIds.isEmpty == false }

    public var node: CascadeNode {
        CascadeNode(
            id: id,
            isActionable: true,
            children: sessionRowIds.map { CascadeNode(id: $0) } + childRowIds.map { CascadeNode(id: $0, isActionable: true) }
        )
    }

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
    public var spoken: String { "Stop \(counted(ports, "port"))" }

    /// The one-line question on the row. It names ports and carries no other count.
    public var confirmation: String {
        ports == 1 ? "Stop 1 port?" : "Stop all \(ports) ports?"
    }

    /// The longer sentence for the tooltip and VoiceOver: ports, projects, and what stays running.
    public var explanation: String {
        var text = "Stop \(counted(ports, "port")) in \(counted(projects, "project"))."
        var left: [String] = []
        if inUse > 0 { left.append("\(counted(inUse, "port")) in use") }
        if otherLeft > 0 { left.append("\(counted(otherLeft, "port")) in Other") }
        if left.isEmpty == false { text += " \(left.joined(separator: " and ")) stay running." }
        if let note = systemNote { text += " \(note)." }
        return text
    }

    public var systemNote: String? {
        systemHidden > 0 ? systemPortsHidden(systemHidden) : nil
    }
}

public enum RunningSubmenuItem: Equatable, Sendable {
    case lastSleep(LastSleepText)
    case keepAwake
    case trackingAgents
    case lidClosed
    case note(String)
    case problem(String)
    case separator
    case nothingRunning
    case project(ProjectRowState)
    case commands([HoldingRow])
    case stopAll(StopAllOffer)

    /// The cascade id of a row the keyboard can act on. Static rows have none.
    public var actionId: String? {
        switch self {
        case .keepAwake: "awake.keep"
        case .trackingAgents: "awake.tracking"
        case .lidClosed: "awake.lid"
        default: nil
        }
    }
}

public struct RunningSubmenuState: Equatable, Sendable {
    public let rows: [ProjectRowState]
    public let commands: [HoldingRow]
    public let stopAll: StopAllOffer?
    public let nothingRunning: Bool
    /// The one line in the problem line's place: why Stop is off, else the Awake problem.
    public let note: String?
    public let lastSleep: LastSleepText?
    private let noteIsProblem: Bool

    /// `problem` is the Awake problem text, shown only when the Core gives no reason of its own.
    public init(running: RunningInput, problem: String? = nil,
        commands: [HoldingRow] = [],
        lastSleep: LastSleepText? = nil
    ) {
        self.commands = commands
        self.lastSleep = lastSleep
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
        var items: [RunningSubmenuItem] = lastSleep.map { [.lastSleep($0), .separator] } ?? []
        items += [.keepAwake, .trackingAgents, .lidClosed]
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
    public let revealPath: String?
    public let up: String?
}

public enum PortMark: Equatable, Sendable {
    case container, process, database, system
}

public struct SessionRowState: Equatable, Sendable {
    public let mark: HoldingMark
    public let title: String
    /// The branch, when the session has one: the only part of the row that may shrink.
    public let branch: String?
    /// `working` or `stopped`: always shown.
    public let state: String
    public let dimmed: Bool
    public let showsDot: Bool
    public let spoken: String

    init(_ session: RunningSessionInput) {
        let shown = session.harness.flatMap(HarnessCopy.display)
        let title = Self.text(session.title) ?? shown ?? "Session"
        let word = session.stopped ? "stopped" : "working"
        let branch = Self.text(session.branch)
        self.title = title
        mark = HarnessCopy.mark(session.harness)
        self.branch = branch
        state = word
        dimmed = session.stopped
        showsDot = session.stopped == false
        var parts = [title]
        if let harness = shown, harness != title { parts.append(harness) }
        if let branch { parts.append("branch \(branch)") }
        parts.append(word)
        spoken = parts.joined(separator: ", ")
    }

    private static func text(_ value: String?) -> String? {
        value.flatMap { $0.isEmpty ? nil : $0 }
    }
}

/// The three things a port row does to the user's desktop, so the flow can run against a fake.
@MainActor
public protocol Desktop: AnyObject {
    func open(_ url: URL)
    func reveal(_ path: String)
}

public enum PortActions {
    public static func openFailure(port: Int) -> String { "Could not open :\(port)" }

    /// Returns nil once the Core's URL is opened, else the sentence to show on the row.
    @MainActor
    public static func open(
        port: Int,
        address: String?,
        desktop: Desktop,
        dispatch: (String) async throws -> String
    ) async -> String? {
        let failure = openFailure(port: port)
        guard let number = UInt16(exactly: port) else { return failure }
        var action: [String: Any] = ["port": number]
        if let address { action["address"] = address }
        guard let request = try? JSONSerialization.data(withJSONObject: ["open": action]),
              let reply = try? await dispatch(String(decoding: request, as: UTF8.self)),
              let url = (try? JSONDecoder().decode(OpenReply.self, from: Data(reply.utf8)))
                  .flatMap({ URL(string: $0.url) }),
              url.scheme?.lowercased() == "http"
        else { return failure }
        desktop.open(url)
        return nil
    }

    @MainActor
    public static func reveal(_ path: String, desktop: Desktop) { desktop.reveal(path) }

    private struct OpenReply: Decodable { let url: String }

    public static func hasOpen(_ port: RunningPortInput) -> Bool { port.openable }
}

public struct PortRowState: Equatable, Sendable {
    public let id: String
    public let port: Int
    public let offersOpen: Bool
    public let mark: PortMark
    public let name: String
    public let suffix: String?
    public let portText: String
    public let tooltip: String?
    public let spoken: String
    public let stop: StopOffer?
    public let target: StopTarget?
    /// Words for the icon actions: tooltip and VoiceOver label. Nil where the action does not exist.
    public let openHelp: String?
    public let stopHelp: String?
    /// The one-line question for a single port's Stop.
    public let confirmation: String?
    private let sentence: String

    init(_ port: RunningPortInput, projectId: String) {
        id = ProjectChildState.portId(project: projectId, port: port)
        self.port = port.port
        offersOpen = PortActions.hasOpen(port)
        name = port.name
        portText = ":\(port.port)"
        suffix = port.container ? "docker" : nil
        stop = port.canStop ? StopOffer(count: 1, request: port.stopRequest) : nil
        target = port.target
        openHelp = offersOpen ? "Open http://localhost:\(port.port)" : nil
        stopHelp = stop == nil ? nil : "Stop \(port.name), port \(port.port)"
        confirmation = stop == nil ? nil : "Stop \(port.name) :\(port.port)?"
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
        if port.container == false, let harness = port.harness.flatMap(HarnessCopy.display) {
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
            revealPath: project.path.flatMap { $0.isEmpty ? nil : $0 },
            up: project.state == .idle ? project.upSinceMs.map { Self.up(nowMs - $0) } : nil
        )
        sessions = project.sessions.map(SessionRowState.init)
        ports = project.ports.map { PortRowState($0, projectId: id) }
        stop = StopOffer(count: project.stoppablePorts, request: project.childStopRequest)
        let hidden = project.ports.filter(\.protectedInGroup).count
        systemNote = project.isOther && hidden > 0
            ? systemPortsHidden(hidden)
            : nil
    }

    public var sections: [ProjectChildSection] {
        (sessions.isEmpty ? [] : [.sessions]) + (ports.isEmpty ? [] : [.ports])
    }

    public static func portId(project: String, port: RunningPortInput) -> String {
        switch port.target {
        case let .process(pid, _): "\(project).port.\(port.port).\(pid)"
        case let .container(id): "\(project).port.\(port.port).\(id)"
        case nil: "\(project).port.\(port.port)"
        }
    }

    public static func stopId(project: String) -> String { "\(project).stop" }

    public static func sessionId(project: String, index: Int) -> String { "\(project).session.\(index)" }

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
