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
    rowStop: String? = nil,
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
        rowStopRequest: rowStop,
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
            project("a", state: .idle, ports: [port(1), port(2)], stoppable: 2, rowStop: "ra", childStop: "ra"),
            project("b", state: .idle, ports: [port(3)], stoppable: 1, rowStop: "rb", childStop: "rb"),
        ]))
        #expect(state.rows[0].stop?.spoken == "Stop 2 ports")
        #expect(state.rows[1].stop?.spoken == "Stop 1 port")
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

    @Test func a_row_offers_stop_only_when_the_input_carries_a_request() {
        let state = RunningSubmenuState(running: running([
            project("idle-one", state: .idle, ports: [port(1, stop: "a"), port(2, stop: "b")],
                    stoppable: 2, rowStop: "req-idle", childStop: "req-idle"),
            project("busy", state: .working, sessions: [live], ports: [port(3, stop: "c")],
                    stoppable: 1, rowStop: nil, childStop: "req-busy"),
            project("mystery", state: .unknown, ports: [port(4)], stoppable: 0),
            project("empty-request", state: .idle, ports: [port(5)], stoppable: 1, rowStop: ""),
        ]))
        #expect(state.rows[0].stop == StopOffer(count: 2, request: "req-idle"))
        #expect(state.rows[1].stop == nil)
        #expect(state.rows[2].stop == nil)
        #expect(state.rows[3].stop == nil)
    }

    @Test func the_child_stop_is_built_from_the_child_request() {
        let state = RunningSubmenuState(running: running([
            project("busy", state: .working, sessions: [live], ports: [port(3, stop: "c")],
                    stoppable: 1, rowStop: nil, childStop: "req-busy"),
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
            "api", state: .idle, ports: [port(1, stop: "a")], stoppable: 1, rowStop: "r", childStop: "r"
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
            project("Other", state: .idle, ports: [port(2, stop: "o")], stoppable: 1, rowStop: "o", isOther: true),
        ], stopAll: [], otherLeft: 1))
        #expect(state.stopAll == nil)
    }

    @Test func stop_all_carries_the_core_counts() {
        let state = RunningSubmenuState(running: running([
            project("a", state: .idle, ports: [port(1)], stoppable: 1, rowStop: "ra"),
            project("b", state: .idle, ports: [port(2), port(3)], stoppable: 2, rowStop: "rb"),
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
