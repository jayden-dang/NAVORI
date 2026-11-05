import Foundation
import NavoriPanel
import Testing

private func port(_ number: Int, stop: String = "") -> RunningPortInput {
    RunningPortInput(port: number, name: "node", stopRequest: stop)
}

private func project(
    _ name: String,
    state: RunningProjectState,
    sessions: [RunningSessionInput] = [],
    ports: [RunningPortInput] = [],
    stoppable: Int = 0,
    childStop: String? = nil,
    isOther: Bool = false
) -> RunningProjectInput {
    RunningProjectInput(
        name: name,
        isOther: isOther,
        state: state,
        sessions: sessions,
        ports: ports,
        stoppablePorts: stoppable,
        childStopRequest: childStop
    )
}

private func running(
    _ projects: [RunningProjectInput],
    unknownReason: String? = nil,
    stopAll: [String] = [],
    stopAllPorts: Int = 0,
    inUse: Int = 0,
    otherLeft: Int = 0,
    hidden: Int = 0
) -> RunningInput {
    RunningInput(
        unknownReason: unknownReason,
        projects: projects,
        hiddenSystemPorts: hidden,
        stopAll: stopAll,
        stopAllPorts: stopAllPorts,
        inUsePorts: inUse,
        otherPortsLeft: otherLeft
    )
}

private let live = RunningSessionInput(stopped: false)
private let ended = RunningSessionInput(stopped: true)

@Suite
struct RunningRowStateTests {
    @Test func a_green_dot_shows_while_a_session_is_live() {
        let row = RunningRowState(running: running([project("klynt", state: .working, sessions: [live])]))
        #expect(row.dot == .green)
        #expect(row.spoken == "Running, active")
        #expect(row.label == "Running")
        #expect(row.hasSubmenu)
    }

    @Test func a_green_dot_shows_while_a_port_is_listed() {
        let row = RunningRowState(running: running([project("api", state: .idle, ports: [port(3000)])]))
        #expect(row.dot == .green)
        #expect(row.spoken == "Running, active")
    }

    @Test func no_dot_shows_while_nothing_runs() {
        let empty = RunningRowState(running: running([]))
        #expect(empty.dot == nil)
        #expect(empty.spoken == "Running, nothing running")
        #expect(empty.hasSubmenu)

        let onlyEnded = RunningRowState(running: running([project("klynt", state: .idle, sessions: [ended])]))
        #expect(onlyEnded.dot == nil)
        #expect(onlyEnded.spoken == "Running, nothing running")
    }

    @Test func a_missing_running_view_has_no_submenu_and_no_dot() {
        let row = RunningRowState(running: nil)
        #expect(row.dot == nil)
        #expect(row.hasSubmenu == false)
        #expect(row.label == "Running")
        #expect(row.spoken == "Running, nothing running")
    }
}

@Suite
struct RunningSubmenuStateTests {
    @Test func a_project_with_only_ended_sessions_and_no_ports_reads_as_nothing_running() {
        let view = running([project("quiet", state: .idle, sessions: [ended])])
        let row = RunningRowState(running: view)
        let state = RunningSubmenuState(running: view)
        #expect(row.dot == nil)
        #expect(state.nothingRunning)
        #expect(state.rows.isEmpty)
        #expect(state.items.contains(.nothingRunning))
    }

    @Test func a_live_session_makes_the_dot_and_the_rows_show() {
        let view = running([project("klynt", state: .working, sessions: [live])])
        let state = RunningSubmenuState(running: view)
        #expect(RunningRowState(running: view).dot == .green)
        #expect(state.nothingRunning == false)
        #expect(state.rows.count == 1)
        #expect(state.items.contains(.nothingRunning) == false)
    }

    @Test func a_listed_port_with_no_session_makes_the_dot_and_the_rows_show() {
        let view = running([project("api", state: .idle, ports: [port(3000)])])
        #expect(RunningRowState(running: view).dot == .green)
        #expect(RunningSubmenuState(running: view).nothingRunning == false)
    }

    @Test func an_empty_view_reads_as_nothing_running_in_both_places() {
        let view = running([])
        #expect(RunningRowState(running: view).dot == nil)
        #expect(RunningSubmenuState(running: view).nothingRunning)
    }

    @Test func the_stop_control_is_spoken_with_its_port_count() {
        let state = RunningSubmenuState(running: running([
            project("a", state: .idle, ports: [port(1), port(2)], stoppable: 2, childStop: "ra"),
            project("b", state: .idle, ports: [port(3)], stoppable: 1, childStop: "rb"),
        ]))
        #expect(state.rows[0].childStop?.spoken == "Stop 2 ports")
    }

    @Test func a_working_project_has_a_filled_dot_and_an_idle_one_a_ring() {
        let state = RunningSubmenuState(running: running([
            project("klynt", state: .working, sessions: [live], ports: [port(3000)]),
            project("api", state: .idle, ports: [port(4000)]),
        ]))
        #expect(state.rows.map(\.dot) == [.filled, .ring])
    }

    @Test func an_unknown_project_has_no_dot() {
        let state = RunningSubmenuState(running: running([project("klynt", state: .unknown, ports: [port(1)])]))
        #expect(state.rows.map(\.dot) == [ProjectDot.none])
    }

    @Test func a_project_row_is_spoken_as_its_state() {
        let state = RunningSubmenuState(running: running([
            project("klynt", state: .working, sessions: [live]),
            project("openklynt-main", state: .idle, ports: [port(1), port(2), port(3)]),
            project("solo", state: .idle, ports: [port(9)]),
            project("quiet", state: .idle, sessions: [ended]),
            project("mystery", state: .unknown, ports: [port(5)]),
        ]))
        #expect(state.rows.map(\.spoken) == [
            "klynt, working",
            "openklynt-main, 3 idle ports",
            "solo, 1 idle port",
            "quiet, idle",
            "mystery, working state unknown",
        ])
        #expect(state.rows.map(\.name) == ["klynt", "openklynt-main", "solo", "quiet", "mystery"])
    }

    @Test func nothing_running_shows_no_stop_all() {
        let state = RunningSubmenuState(running: running([]))
        #expect(state.nothingRunning)
        #expect(state.stopAll == nil)
        #expect(state.items.contains(.nothingRunning))
        #expect(state.items.contains { if case .stopAll = $0 { true } else { false } } == false)
    }

    @Test func the_note_line_shows_only_while_a_reason_is_set() {
        let reason = "Tracking agents is off, so Stop is off."
        let unknown = RunningSubmenuState(
            running: running([project("klynt", state: .unknown, ports: [port(1)])], unknownReason: reason)
        )
        #expect(unknown.note == reason)
        #expect(unknown.items.contains(.note(reason)))

        let known = RunningSubmenuState(running: running([project("klynt", state: .working, sessions: [live])]))
        #expect(known.note == nil)
    }

    @Test func the_child_stop_is_built_from_the_child_request() {
        let state = RunningSubmenuState(running: running([
            project("busy", state: .working, sessions: [live], ports: [port(3, stop: "c")],
                    stoppable: 1, childStop: "req-busy"),
            project("bare", state: .working, sessions: [live]),
        ]))
        #expect(state.rows[0].childStop == StopOffer(count: 1, request: "req-busy"))
        #expect(state.rows[1].childStop == nil)
    }

    @Test func a_port_offers_stop_only_when_it_carries_a_request() {
        let ports = [port(1, stop: "a"), port(2, stop: "")]
        #expect(ports.map(\.canStop) == [true, false])
    }

    @Test func the_submenu_runs_from_keep_awake_to_stop_all() {
        let idle = project(
            "api", state: .idle, ports: [port(1, stop: "a")], stoppable: 1, childStop: "r"
        )
        let state = RunningSubmenuState(
            running: running(
                [idle], unknownReason: "Awake cannot be read, so Stop is off.",
                stopAll: ["r"], stopAllPorts: 1, inUse: 2, otherLeft: 3, hidden: 4
            )
        )
        #expect(state.items == [
            .keepAwake,
            .trackingAgents,
            .note("Awake cannot be read, so Stop is off."),
            .separator,
            .project(state.rows[0]),
            .separator,
            .stopAll(StopAllOffer(requests: ["r"], ports: 1, projects: 1, inUse: 2, otherLeft: 3, systemHidden: 4)),
        ])
    }

    @Test func no_note_means_no_problem_line_and_no_stop_all_means_no_closing_separator() {
        let state = RunningSubmenuState(running: running([project("klynt", state: .working, sessions: [live])]))
        #expect(state.items == [
            .keepAwake, .trackingAgents, .separator, .project(state.rows[0]),
        ])
    }

    @Test func an_awake_problem_takes_the_problem_line_when_the_project_states_are_known() {
        let state = RunningSubmenuState(
            running: running([project("klynt", state: .working, sessions: [live])]),
            problem: "Awake could not read its leases."
        )
        #expect(state.items.contains(.problem("Awake could not read its leases.")))
    }

    @Test func the_same_text_from_each_source_keeps_its_own_kind_of_line() {
        let text = "Tracking agents is off, so Stop is off."
        let fromCore = RunningSubmenuState(
            running: running([project("klynt", state: .unknown, ports: [port(1)])], unknownReason: text),
            problem: text
        )
        #expect(fromCore.items.contains(.note(text)))
        #expect(fromCore.items.contains(.problem(text)) == false)

        let fromAwake = RunningSubmenuState(
            running: running([project("klynt", state: .working, sessions: [live])]),
            problem: text
        )
        #expect(fromAwake.items.contains(.problem(text)))
        #expect(fromAwake.items.contains(.note(text)) == false)
    }

    @Test func stop_all_is_absent_when_no_named_project_is_idle() {
        let state = RunningSubmenuState(running: running([
            project("busy", state: .working, sessions: [live], ports: [port(1)]),
            project("Other", state: .idle, ports: [port(2, stop: "o")], stoppable: 1, isOther: true),
        ], stopAll: [], otherLeft: 1))
        #expect(state.stopAll == nil)
    }

    @Test func stop_all_carries_the_core_counts() {
        let state = RunningSubmenuState(running: running([
            project("a", state: .idle, ports: [port(1)], stoppable: 1),
            project("b", state: .idle, ports: [port(2), port(3)], stoppable: 2),
        ], stopAll: ["ra", "rb"], stopAllPorts: 3, inUse: 5, otherLeft: 1, hidden: 2))
        #expect(state.stopAll == StopAllOffer(
            requests: ["ra", "rb"], ports: 3, projects: 2, inUse: 5, otherLeft: 1, systemHidden: 2
        ))
    }
}

@Suite
struct ProjectRowIdTests {
    @Test func a_project_row_id_carries_its_name_and_other_has_its_own() {
        let state = RunningSubmenuState(running: running([
            project("klynt", state: .working, sessions: [live]),
            project("Other", state: .idle, ports: [port(2)], isOther: true),
        ]))
        #expect(state.rows.map(\.id) == ["running.project.klynt", "running.other"])
    }

    @Test func a_project_that_is_named_other_keeps_a_distinct_id() {
        let state = RunningSubmenuState(running: running([
            project("Other", state: .idle, ports: [port(1)]),
            project("Other", state: .idle, ports: [port(2)], isOther: true),
        ]))
        #expect(state.rows.map(\.id) == ["running.project.Other", "running.other"])
    }
}

private func child(
    _ name: String = "klynt",
    state: RunningProjectState = .idle,
    sessions: [RunningSessionInput] = [],
    ports: [RunningPortInput] = [],
    stoppable: Int = 0,
    childStop: String? = nil,
    path: String? = nil,
    upSinceMs: Int64? = nil,
    isOther: Bool = false,
    nowMs: Int64 = 0,
    home: String = "/Users/me"
) -> ProjectChildState {
    ProjectChildState(
        project: RunningProjectInput(
            name: name,
            isOther: isOther,
            state: state,
            sessions: sessions,
            ports: ports,
            stoppablePorts: stoppable,
            childStopRequest: childStop,
            path: path,
            upSinceMs: upSinceMs
        ),
        nowMs: nowMs,
        home: home
    )
}

private func session(
    _ title: String?,
    harness: String? = "Claude Code",
    branch: String? = "dev",
    stopped: Bool = false
) -> RunningSessionInput {
    RunningSessionInput(stopped: stopped, harness: harness, title: title, branch: branch)
}

private func listener(
    _ number: Int,
    _ name: String,
    stop: String = "{}",
    container: Bool = false,
    kind: String = "dev_server",
    worktree: String? = nil,
    harness: String? = nil,
    protectedInGroup: Bool = false
) -> RunningPortInput {
    RunningPortInput(
        port: number,
        name: name,
        stopRequest: stop,
        kind: kind,
        worktree: worktree,
        harness: harness,
        container: container,
        protectedInGroup: protectedInGroup
    )
}

private let hour: Int64 = 3_600_000
private let day: Int64 = 24 * hour

@Suite
struct ProjectChildStateTests {
    @Test func the_header_shows_up_only_for_a_project_with_no_working_session() {
        let since = Int64(1_000)
        let idle = child(
            state: .idle, ports: [listener(1, "node")], path: "/Users/me/Developer/klynt",
            upSinceMs: since, nowMs: since + 2 * day + 20 * hour + 5
        )
        #expect(idle.header.name == "klynt")
        #expect(idle.header.path == "~/Developer/klynt")
        #expect(idle.header.up == "up 2d 20h")

        let working = child(
            state: .working, sessions: [session("a")], ports: [listener(1, "node")],
            path: "/Users/me/Developer/klynt", upSinceMs: since, nowMs: since + 2 * day
        )
        #expect(working.header.up == nil)
        #expect(working.header.path == "~/Developer/klynt")

        let unknown = child(state: .unknown, ports: [listener(1, "node")], upSinceMs: since, nowMs: since + day)
        #expect(unknown.header.up == nil)
    }

    @Test func up_reads_in_the_two_largest_units() {
        func up(_ ms: Int64) -> String? {
            child(state: .idle, ports: [listener(1, "n")], upSinceMs: 0, nowMs: ms).header.up
        }
        #expect(up(day + 30 * 60_000) == "up 1d 0h")
        #expect(up(5 * hour + 12 * 60_000) == "up 5h 12m")
        #expect(up(12 * 60_000) == "up 12m")
        #expect(up(10_000) == "up <1m")
    }

    @Test func a_path_outside_the_home_folder_is_kept_whole() {
        #expect(child(path: "/opt/work/klynt", home: "/Users/me").header.path == "/opt/work/klynt")
        #expect(child(path: "/Users/me2/x", home: "/Users/me").header.path == "/Users/me2/x")
        #expect(child(path: "/Users/me", home: "/Users/me").header.path == "~")
        #expect(child(path: nil).header.path == nil)
    }

    @Test func a_section_with_no_rows_is_left_out() {
        let portsOnly = child(ports: [listener(1, "node")])
        #expect(portsOnly.sessions.isEmpty)
        #expect(portsOnly.sections == [.ports])
        let sessionsOnly = child(state: .working, sessions: [session("a")])
        #expect(sessionsOnly.ports.isEmpty)
        #expect(sessionsOnly.sections == [.sessions])
        #expect(child(state: .working, sessions: [session("a")], ports: [listener(1, "n")]).sections == [.sessions, .ports])
        #expect(child().sections.isEmpty)
    }

    @Test func a_working_project_still_has_stop_n_ports_in_its_child() {
        let state = child(
            state: .working, sessions: [session("a")], ports: [listener(1, "n"), listener(2, "m")],
            stoppable: 2, childStop: "child-req"
        )
        #expect(state.stop == StopOffer(count: 2, request: "child-req"))
        #expect(state.stop?.spoken == "Stop 2 ports")
        #expect(child(ports: [listener(1, "n")], stoppable: 0, childStop: nil).stop == nil)
        #expect(child(ports: [listener(1, "n")], stoppable: 1, childStop: "").stop == nil)
    }

    @Test func other_says_how_many_system_ports_are_hidden() {
        let ports = [
            listener(1, "node"),
            listener(5432, "postgres", kind: "database", protectedInGroup: true),
            listener(88, "sshd", kind: "system", protectedInGroup: true),
        ]
        #expect(child("Other", ports: ports, isOther: true).systemNote == "2 system ports hidden")
        #expect(child("Other", ports: [ports[1]], isOther: true).systemNote == "1 system port hidden")
        #expect(child("Other", ports: [ports[0]], isOther: true).systemNote == nil)
        #expect(child("klynt", ports: ports).systemNote == nil)
    }

    @Test func every_row_reads_as_one_sentence_with_its_state_word() {
        let state = child(
            state: .working,
            sessions: [
                session("Parallel roads", branch: "dev"),
                session("old", harness: "Codex", branch: nil, stopped: true),
                session(nil, harness: nil, branch: nil),
            ],
            ports: [listener(10086, "kimi-webbridge")]
        )
        #expect(state.sessions[0].spoken == "Parallel roads, Claude Code, branch dev, working")
        #expect(state.sessions[1].spoken == "old, Codex, stopped")
        #expect(state.sessions[2].spoken == "Session, working")
        #expect(state.sessions[0].detail == "dev \u{00B7} working")
        #expect(state.sessions[1].detail == "stopped")
        #expect(state.sessions.map(\.dimmed) == [false, true, false])
        #expect(state.sessions.map(\.showsDot) == [true, false, true])
        #expect(state.ports[0].spoken == "kimi-webbridge, port 10086, listening")
        #expect(state.ports[0].portText == ":10086")
    }

    @Test func a_port_row_names_its_kind_and_a_container_says_docker() {
        let state = child(ports: [
            listener(4318, "klynt-otel", container: true, kind: "docker"),
            listener(5432, "klynt-postgres", container: true, kind: "database"),
            listener(3000, "node"),
            listener(5433, "postgres", kind: "database"),
        ])
        #expect(state.ports.map(\.mark) == [.container, .container, .process, .database])
        #expect(state.ports.map(\.suffix) == ["docker", "docker", nil, nil])
    }

    @Test func a_port_stop_uses_the_pid_request_for_a_process_and_the_project_request_for_a_container() {
        let pidRequest = #"{"kill":{"selector":{"pid":412},"targets":[]}}"#
        let projectRequest = #"{"kill":{"selector":{"project":"klynt"},"targets":[]}}"#
        let state = child(ports: [
            listener(3000, "node", stop: pidRequest),
            listener(6379, "klynt-redis", stop: projectRequest, container: true, kind: "docker"),
            listener(5432, "klynt-postgres", stop: "", container: true, kind: "database", protectedInGroup: true),
        ])
        #expect(state.ports[0].stop == StopOffer(count: 1, request: pidRequest))
        #expect(state.ports[1].stop == StopOffer(count: 1, request: projectRequest))
        #expect(state.ports[2].stop == nil)
        #expect(state.ports[0].stop?.spoken == "Stop 1 port")
    }

    @Test func a_port_rows_tooltip_and_spoken_sentence_name_its_worktree_and_harness_and_none_for_docker() {
        let state = child(ports: [
            listener(3000, "next-server", worktree: "feat-x", harness: "claude_code"),
            listener(3001, "vite", harness: "codex"),
            listener(3002, "bun", worktree: "main"),
            listener(6379, "klynt-redis", container: true, kind: "docker"),
        ])
        #expect(state.ports[0].tooltip == "worktree feat-x \u{00B7} started by Claude Code")
        #expect(state.ports[0].spoken == "next-server, port 3000, worktree feat-x, started by Claude Code, listening")
        #expect(state.ports[1].tooltip == "started by Codex")
        #expect(state.ports[2].tooltip == "worktree main")
        #expect(state.ports[3].tooltip == nil)
        #expect(state.ports[3].spoken == "klynt-redis, docker, port 6379, listening")
    }

    @Test func a_stopping_or_failed_port_speaks_its_state_word() {
        let row = child(ports: [listener(3000, "node")]).ports[0]
        #expect(row.spoken(for: .stopping) == "node, port 3000, stopping")
        #expect(row.spoken(for: .failed("The process changed.")) == "node, port 3000, stop failed, The process changed.")
        #expect(row.spoken(for: nil) == row.spoken)
    }

    @Test func the_rows_carry_ids_under_their_project_and_the_offers_by_id() {
        let state = RunningSubmenuState(running: running([
            project(
                "klynt", state: .idle,
                ports: [port(3000, stop: "a"), port(5432, stop: "")],
                stoppable: 1, childStop: "c"
            ),
        ]))
        let row = state.rows[0]
        #expect(row.childRowIds == ["running.project.klynt.port.3000", "running.project.klynt.port.5432", "running.project.klynt.stop"])
        #expect(row.childOffers["running.project.klynt.port.3000"] == StopOffer(count: 1, request: "a"))
        #expect(row.childOffers["running.project.klynt.port.5432"] == nil)
        #expect(row.childOffers["running.project.klynt.stop"] == StopOffer(count: 1, request: "c"))
    }

    @Test func the_other_child_ends_with_the_note_and_a_session_less_project_has_no_sessions_section() {
        let state = child("Other", ports: [listener(88, "sshd", kind: "system", protectedInGroup: true)], isOther: true)
        #expect(state.sections == [.ports])
        #expect(state.systemNote == "1 system port hidden")
    }
}

@Suite
struct ProjectChildReachTests {
    @Test func a_project_with_sessions_and_no_ports_opens_a_child_of_session_rows() {
        let state = RunningSubmenuState(running: running([
            project("NAVORI", state: .working, sessions: [live, ended]),
        ]))
        let row = state.rows[0]
        #expect(row.hasChild)
        let node = row.node
        #expect(node.id == row.id)
        #expect(node.children.count == 2)
        #expect(node.children.allSatisfy { $0.isActionable == false })
        #expect(row.childRowIds.isEmpty)
    }

    @Test func the_sessions_only_child_shows_only_the_sessions_section() {
        let state = ProjectChildState(
            project: project("NAVORI", state: .working, sessions: [live]), nowMs: 0, home: "/Users/x"
        )
        #expect(state.sections == [.sessions])
        #expect(state.stop == nil)
        #expect(state.header.path == nil)
        #expect(state.header.up == nil)
    }

    @Test func a_project_with_ports_nests_sessions_then_ports_then_stop() {
        let state = RunningSubmenuState(running: running([
            project("klynt", state: .working, sessions: [live], ports: [port(3000, stop: "a")], stoppable: 1, childStop: "c"),
        ]))
        let node = state.rows[0].node
        #expect(node.children.map(\.id) == [
            "running.project.klynt.session.0", "running.project.klynt.port.3000", "running.project.klynt.stop",
        ])
        #expect(node.children.map(\.isActionable) == [false, true, true])
    }

    @Test func other_with_sessions_only_opens_too() {
        let state = RunningSubmenuState(running: running([
            project("Other", state: .working, sessions: [live], isOther: true),
        ]))
        #expect(state.rows[0].hasChild)
        #expect(state.rows[0].node.id == "running.other")
        #expect(state.rows[0].node.children.count == 1)
    }

    @Test func a_project_with_neither_sessions_nor_ports_has_no_child_and_so_no_chevron() {
        let state = RunningSubmenuState(running: running([
            project("klynt", state: .working, sessions: [live]),
            project("empty", state: .unknown),
        ]))
        let empty = state.rows[1]
        #expect(empty.hasChild == false)
        #expect(empty.node.children.isEmpty)
    }

    @Test func the_chevron_follows_the_node_rule_exactly() {
        let state = RunningSubmenuState(running: running([
            project("a", state: .working, sessions: [live]),
            project("b", state: .idle, ports: [port(1)]),
            project("c", state: .unknown),
        ]))
        for row in state.rows {
            #expect(row.hasChild == (row.node.children.isEmpty == false))
        }
    }
}

@Suite
struct PortRowIdTests {
    @Test func two_listeners_on_one_port_keep_distinct_ids_and_offers() {
        let first = RunningPortInput(port: 3000, name: "node", stopRequest: "a", target: .process(pid: 10, startUs: 1))
        let second = RunningPortInput(port: 3000, name: "node", stopRequest: "b", target: .process(pid: 11, startUs: 2))
        let box = RunningPortInput(port: 3000, name: "web", stopRequest: "c", target: .container(id: "abc"))
        let state = RunningSubmenuState(running: running([
            RunningProjectInput(
                name: "p", isOther: false, state: .idle, sessions: [], ports: [first, second, box],
                stoppablePorts: 3, childStopRequest: "c"
            ),
        ]))
        let ids = state.rows[0].childRowIds
        #expect(Set(ids).count == ids.count)
        #expect(state.rows[0].childOffers.count == 4)
        #expect(ProjectChildState.portId(project: "p", port: first) != ProjectChildState.portId(project: "p", port: second))
    }
}

@Suite
struct RunningCommandRowsTests {
    @Test func commands_show_as_rows_between_the_projects_and_stop_all() {
        let commands = AwakeSubmenuState(
            status: .on(sources: []),
            leases: [AwakeLeaseInput(kind: .command, harness: nil, label: "make test", pid: 77, stopped: false, until: nil)],
            displayHeld: false,
            tracking: true
        ).commandRows
        #expect(commands.map(\.name) == ["make test"])
        let state = RunningSubmenuState(
            running: running(
                [project("api", state: .idle, ports: [port(1, stop: "a")], stoppable: 1, childStop: "r")],
                stopAll: ["r"], stopAllPorts: 1
            ),
            commands: commands
        )
        let kinds = state.items.map { item -> String in
            switch item {
            case .project: "project"
            case .commands: "commands"
            case .stopAll: "stopAll"
            case .separator: "separator"
            default: "other"
            }
        }
        #expect(kinds == ["other", "other", "separator", "project", "separator", "commands", "separator", "stopAll"])
    }

    @Test func no_commands_leave_the_list_as_it_was() {
        let state = RunningSubmenuState(running: running([project("api", state: .working, sessions: [live])]))
        #expect(state.items.contains { if case .commands = $0 { true } else { false } } == false)
    }
}

@Suite
struct PortActionsTests {
    private func child(_ ports: [RunningPortInput]) -> ProjectChildState {
        ProjectChildState(
            project: RunningProjectInput(
                name: "api",
                isOther: false,
                state: .idle,
                sessions: [],
                ports: ports,
                stoppablePorts: 0,
                childStopRequest: nil
            ),
            nowMs: 0,
            home: "/Users/me"
        )
    }

    @Test func open_is_offered_only_where_the_core_says_the_port_is_openable() {
        let state = child([
            RunningPortInput(port: 3000, name: "node", stopRequest: "", openable: true),
            RunningPortInput(port: 5432, name: "postgres", stopRequest: "", openable: false),
        ])
        #expect(state.ports.map(\.offersOpen) == [true, false])
        #expect(PortActions.hasOpen(RunningPortInput(port: 1, name: "a", stopRequest: "", openable: true)))
        #expect(PortActions.hasOpen(RunningPortInput(port: 1, name: "a", stopRequest: "", openable: false)) == false)
    }

    @Test func copy_text_is_localhost_and_the_port() {
        #expect(PortActions.copyText(3000) == "localhost:3000")
        let state = child([RunningPortInput(port: 8080, name: "node", stopRequest: "")])
        #expect(state.ports[0].copyText == "localhost:8080")
    }

    @Test func the_header_reveals_the_full_path_while_showing_it_abbreviated() {
        let state = ProjectChildState(
            project: RunningProjectInput(
                name: "api",
                isOther: false,
                state: .idle,
                sessions: [],
                ports: [],
                stoppablePorts: 0,
                childStopRequest: nil,
                path: "/Users/me/dev/api"
            ),
            nowMs: 0,
            home: "/Users/me"
        )
        #expect(state.header.path == "~/dev/api")
        #expect(state.header.revealPath == "/Users/me/dev/api")
    }
}

@MainActor
private final class FakeDesktop: Desktop {
    var opened: [URL] = []
    var copied: [String] = []
    var revealed: [String] = []

    func open(_ url: URL) { opened.append(url) }
    func copy(_ text: String) { copied.append(text) }
    func reveal(_ path: String) { revealed.append(path) }
}

private struct Unreachable: Error {}

@MainActor
@Suite
struct PortDesktopActionTests {
    @Test func open_dispatches_the_open_action_and_opens_the_url_the_core_returns() async throws {
        let desktop = FakeDesktop()
        var sent: [String] = []
        let failure = await PortActions.open(port: 3000, address: nil, desktop: desktop) { request in
            sent.append(request)
            return #"{"url":"http://localhost:3000"}"#
        }
        #expect(failure == nil)
        #expect(desktop.opened == [URL(string: "http://localhost:3000")!])
        let object = try #require(JSONSerialization.jsonObject(with: Data(sent[0].utf8)) as? [String: [String: Int]])
        #expect(object == ["open": ["port": 3000]])
    }

    @Test func open_names_the_address_when_one_is_given() async throws {
        let desktop = FakeDesktop()
        var sent = ""
        _ = await PortActions.open(port: 80, address: "10.0.0.2", desktop: desktop) { request in
            sent = request
            return #"{"url":"http://10.0.0.2:80"}"#
        }
        let object = try #require(JSONSerialization.jsonObject(with: Data(sent.utf8)) as? [String: [String: Any]])
        #expect(object["open"]?["address"] as? String == "10.0.0.2")
    }

    @Test func open_that_fails_gives_one_fixed_sentence_and_opens_nothing() async {
        let replies: [@Sendable (String) async throws -> String] = [
            { _ in throw Unreachable() },
            { _ in "{}" },
            { _ in #"{"url":""}"# },
            { _ in #"{"url":"file:///etc/passwd"}"# },
            { _ in #"{"url":"javascript:alert(1)"}"# },
            { _ in #"{"url":"ftp://localhost:3000"}"# },
            { _ in #"{"url":"https://localhost:3000"}"# },
        ]
        for reply in replies {
            let desktop = FakeDesktop()
            let failure = await PortActions.open(port: 3000, address: nil, desktop: desktop, dispatch: reply)
            #expect(failure == "Could not open :3000")
            #expect(desktop.opened.isEmpty)
        }
    }

    @Test func an_upper_case_http_scheme_is_opened() async {
        let desktop = FakeDesktop()
        let failure = await PortActions.open(port: 3000, address: nil, desktop: desktop) { _ in
            #"{"url":"HTTP://localhost:3000"}"#
        }
        #expect(failure == nil)
        #expect(desktop.opened == [URL(string: "HTTP://localhost:3000")!])
    }

    @Test func a_port_outside_the_port_range_is_not_sent() async {
        let desktop = FakeDesktop()
        var sent = false
        let failure = await PortActions.open(port: 70_000, address: nil, desktop: desktop) { _ in
            sent = true
            return ""
        }
        #expect(failure == "Could not open :70000")
        #expect(sent == false)
    }

    @Test func copy_puts_localhost_and_the_port_on_the_pasteboard() {
        let desktop = FakeDesktop()
        PortActions.copy(port: 3000, desktop: desktop)
        #expect(desktop.copied == ["localhost:3000"])
        #expect(desktop.opened.isEmpty)
    }

    @Test func reveal_shows_the_full_path() {
        let desktop = FakeDesktop()
        PortActions.reveal("/Users/me/dev/api", desktop: desktop)
        #expect(desktop.revealed == ["/Users/me/dev/api"])
    }
}
