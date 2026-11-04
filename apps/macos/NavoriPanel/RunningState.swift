public enum RunningProjectState: Equatable, Sendable {
    case working, idle, unknown
}

public struct RunningSessionInput: Equatable, Sendable {
    public let stopped: Bool

    public init(stopped: Bool) {
        self.stopped = stopped
    }
}

public struct RunningPortInput: Equatable, Sendable {
    public let port: Int
    public let name: String
    /// Empty when the Core offers no Stop for this port.
    public let stopRequest: String

    public init(port: Int, name: String, stopRequest: String) {
        self.port = port
        self.name = name
        self.stopRequest = stopRequest
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

    public init(
        name: String,
        isOther: Bool,
        state: RunningProjectState,
        sessions: [RunningSessionInput],
        ports: [RunningPortInput],
        stoppablePorts: Int,
        rowStopRequest: String?,
        childStopRequest: String?
    ) {
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

    init?(count: Int, request: String?) {
        guard let request, request.isEmpty == false else { return nil }
        self.init(count: count, request: request)
    }
}

public struct ProjectRowState: Equatable, Sendable {
    public let name: String
    public let dot: ProjectDot
    public let spoken: String
    /// `Stop N` on the row. Present only when the Core sent a row request.
    public let stop: StopOffer?
    /// `Stop N ports` in the child menu. Present only when the Core sent a child request.
    public let childStop: StopOffer?

    init(_ project: RunningProjectInput) {
        name = project.name
        stop = StopOffer(count: project.stoppablePorts, request: project.rowStopRequest)
        childStop = StopOffer(count: project.stoppablePorts, request: project.childStopRequest)
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

public enum RunningSubmenuItem: Equatable, Sendable {
    case keepAwake
    case trackingAgents
    case note(String)
    case separator
    case nothingRunning
    case project(ProjectRowState)
    case stopAll(StopAllOffer)
}

public struct RunningSubmenuState: Equatable, Sendable {
    public let rows: [ProjectRowState]
    public let stopAll: StopAllOffer?
    public let nothingRunning: Bool
    /// The one line in the problem line's place: why Stop is off, else the Awake problem.
    public let note: String?

    /// `problem` is the Awake problem text, shown only when the Core gives no reason of its own.
    public init(running: RunningInput, problem: String? = nil) {
        nothingRunning = running.isActive == false
        rows = nothingRunning ? [] : running.projects.map(ProjectRowState.init)
        note = [running.unknownReason, problem].lazy.compactMap { $0 }.first { $0.isEmpty == false }
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
        if let note { items.append(.note(note)) }
        items.append(.separator)
        items += nothingRunning ? [.nothingRunning] : rows.map(RunningSubmenuItem.project)
        if let stopAll { items += [.separator, .stopAll(stopAll)] }
        return items
    }
}
