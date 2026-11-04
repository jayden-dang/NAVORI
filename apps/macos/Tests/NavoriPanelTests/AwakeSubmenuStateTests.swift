import NavoriPanel
import Testing

@Suite
struct AwakeSubmenuStateTests {
    private static func session(
        _ harness: String?,
        _ label: String?,
        pid: Int32,
        stopped: Bool = false
    ) -> AwakeLeaseInput {
        AwakeLeaseInput(kind: .session, harness: harness, label: label, pid: pid, stopped: stopped, until: nil)
    }

    private static func state(
        status: AwakeStatus = .on(sources: []),
        leases: [AwakeLeaseInput] = [],
        displayHeld: Bool = false,
        actionFailure: String? = nil
    ) -> AwakeSubmenuState {
        AwakeSubmenuState(status: status, leases: leases, displayHeld: displayHeld, actionFailure: actionFailure)
    }

    private static let hour = AwakeLeaseInput(
        kind: .duration, harness: nil, label: "1 hour", pid: nil, stopped: false, until: "11:38"
    )
    private static let untilOff = AwakeLeaseInput(
        kind: .untilOff, harness: nil, label: "Until off", pid: nil, stopped: false, until: nil
    )
    private static let display = AwakeLeaseInput(
        kind: .display, harness: nil, label: "Display", pid: nil, stopped: false, until: nil
    )
    private static let command = AwakeLeaseInput(
        kind: .command, harness: nil, label: "cargo", pid: 23719, stopped: false, until: nil
    )

    @Test func the_menu_hold_is_one_hour() {
        #expect(AwakeSubmenuState.menuHoldSeconds == 3600)
    }

    @Test func nothing_wrong_means_no_status_text() {
        #expect(Self.state(status: .off).problem == nil)
        #expect(Self.state(leases: [Self.hour]).problem == nil)
        #expect(Self.state(leases: [Self.display], displayHeld: true).problem == nil)
    }

    @Test func a_refused_system_assertion_is_a_problem() {
        let state = Self.state(status: .notHeld, leases: [Self.hour])
        #expect(state.problem == AwakeProblem(
            title: "Can't hold this Mac awake",
            detail: "macOS refused the request. Navori keeps trying."
        ))
    }

    @Test func a_refused_display_assertion_is_a_problem() {
        let state = Self.state(leases: [Self.hour, Self.display], displayHeld: false)
        #expect(state.problem == AwakeProblem(
            title: "Display not held",
            detail: "2 Leases · idle sleep blocked"
        ))
    }

    @Test func a_recorded_problem_is_written_in_the_submenu() {
        let text = "Awake could not save a lease. Navori retries in 60 seconds."
        let state = Self.state(status: .problem(text, held: true), leases: [Self.hour])
        #expect(state.problem == AwakeProblem(title: text, detail: nil))
    }

    @Test func a_failed_menu_action_is_written_in_the_submenu() {
        let state = Self.state(status: .off, actionFailure: "The Navori core is not running.")
        #expect(state.problem == AwakeProblem(title: "The Navori core is not running.", detail: nil))
    }

    @Test func a_failed_menu_action_keeps_the_refusal_it_happened_during() {
        let state = Self.state(status: .notHeld, leases: [Self.hour], actionFailure: "The Navori core is not running.")
        #expect(state.problem == AwakeProblem(
            title: "Can't hold this Mac awake",
            detail: "The Navori core is not running."
        ))
    }

    @Test func commands_are_checked_by_the_lease_they_made() {
        let off = Self.state(status: .off)
        #expect(off.hourChecked == false)
        #expect(off.hourUntil == nil)
        #expect(off.untilOffChecked == false)
        #expect(off.displayChecked == false)
        #expect(off.releaseAllEnabled == false)

        let hour = Self.state(leases: [Self.hour])
        #expect(hour.hourChecked)
        #expect(hour.hourUntil == "until 11:38")
        #expect(hour.untilOffChecked == false)
        #expect(hour.releaseAllEnabled)

        let rest = Self.state(leases: [Self.untilOff, Self.display], displayHeld: true)
        #expect(rest.hourChecked == false)
        #expect(rest.untilOffChecked)
        #expect(rest.displayChecked)
    }

    @Test func holding_groups_run_held_by_you_then_commands_then_projects_alphabetically() {
        let state = Self.state(leases: [
            Self.session("Claude Code", "skills/shape-video", pid: 1),
            Self.session("Claude Code", "NAVORI/main", pid: 2),
            Self.command,
            Self.session("Claude Code", "klynt/dev", pid: 3),
            Self.hour,
        ])
        #expect(state.holdingGroups.map(\.title) == ["Held by you", "Commands", "klynt", "NAVORI", "skills"])
    }

    @Test func a_project_lists_working_sessions_first_and_keeps_its_order_while_states_change() {
        let state = Self.state(leases: [
            Self.session("Claude Code", "klynt/road128", pid: 70190, stopped: true),
            Self.session("Claude Code", "klynt/dev", pid: 51865, stopped: true),
            Self.session("Claude Code", "klynt/dev", pid: 19801, stopped: true),
            Self.session("Claude Code", "klynt/dev", pid: 22560),
        ])
        let rows = state.holdingGroups[0].rows
        #expect(rows.map(\.name) == ["dev", "dev", "dev", "road128"])
        #expect(rows.map(\.word) == ["working", "stopped", "stopped", "stopped"])
        #expect(rows.map(\.help) == [
            "Claude Code · klynt/dev · pid 22560",
            "Claude Code · klynt/dev · pid 19801",
            "Claude Code · klynt/dev · pid 51865",
            "Claude Code · klynt/road128 · pid 70190",
        ])
    }

    @Test(arguments: [
        ("Claude Code", HoldingMark.claude),
        ("Codex", HoldingMark.codex),
        ("Grok", HoldingMark.grok),
        ("Somebody Else", HoldingMark.terminal),
    ])
    func a_session_leads_with_its_harness_mark(_ harness: String, _ mark: HoldingMark) {
        let state = Self.state(leases: [Self.session(harness, "NAVORI/main", pid: 1)])
        #expect(state.holdingGroups[0].rows[0].mark == mark)
    }

    @Test func a_working_session_shows_the_dot_and_a_stopped_one_steps_back() {
        let state = Self.state(leases: [
            Self.session("Codex", "NAVORI/main", pid: 1),
            Self.session("Codex", "NAVORI/main", pid: 2, stopped: true),
        ])
        let rows = state.holdingGroups[0].rows
        #expect(rows[0].showsDot)
        #expect(rows[0].isQuiet == false)
        #expect(rows[0].accessibilitySentence == "Codex, NAVORI/main, working, pid 1")
        #expect(rows[1].showsDot == false)
        #expect(rows[1].isQuiet)
        #expect(rows[1].accessibilitySentence == "Codex, NAVORI/main, stopped, pid 2")
    }

    @Test func a_label_with_no_branch_is_its_own_project() {
        let row = Self.state(leases: [Self.session("Grok", "scratch", pid: 1)]).holdingGroups[0]
        #expect(row.title == "scratch")
        #expect(row.rows[0].name == "scratch")
    }

    @Test func a_branch_with_a_slash_stays_whole() {
        let group = Self.state(leases: [Self.session("Grok", "NAVORI/feat/menu", pid: 1)]).holdingGroups[0]
        #expect(group.title == "NAVORI")
        #expect(group.rows[0].name == "feat/menu")
    }

    @Test func a_session_with_no_label_is_named_by_its_harness() {
        let group = Self.state(leases: [Self.session("Codex", nil, pid: 1)]).holdingGroups[0]
        #expect(group.title == "Other")
        #expect(group.rows[0].name == "Codex")
    }

    @Test func manual_and_command_rows_say_what_they_are() {
        let state = Self.state(leases: [Self.display, Self.command, Self.hour, Self.untilOff], displayHeld: true)
        let manual = state.holdingGroups[0].rows
        #expect(manual.map(\.name) == ["1 hour", "Until off", "Display"])
        #expect(manual.map(\.word) == ["until 11:38", "", ""])
        #expect(manual.map(\.mark) == [.clock, .infinity, .display])
        #expect(manual.allSatisfy { $0.showsDot == false && $0.isQuiet == false })
        #expect(manual[0].accessibilitySentence == "1 hour, until 11:38")
        let command = state.holdingGroups[1].rows[0]
        #expect(command.name == "cargo")
        #expect(command.word == "running")
        #expect(command.mark == .terminal)
        #expect(command.help == "pid 23719")
        #expect(command.accessibilitySentence == "cargo, running, pid 23719")
    }

    @Test func a_duration_without_a_label_does_not_claim_one_hour() {
        let state = Self.state(leases: [
            AwakeLeaseInput(kind: .duration, harness: nil, label: nil, pid: nil, stopped: false, until: "11:38")
        ])
        #expect(state.holdingGroups[0].rows[0].name == "Manual")
    }

    @Test func a_manual_hold_steps_back_while_the_mac_is_not_held() {
        let state = Self.state(status: .notHeld, leases: [Self.hour])
        #expect(state.holdingGroups[0].rows[0].isQuiet)
    }

    @Test func no_text_says_the_mac_will_sleep_or_names_another_app() {
        let states = [
            Self.state(status: .off),
            Self.state(status: .notHeld, leases: [Self.hour]),
            Self.state(leases: [Self.hour, Self.untilOff, Self.display, Self.command]),
            Self.state(leases: [Self.session("Claude Code", "NAVORI/main", pid: 1, stopped: true)]),
        ]
        let texts = states.flatMap { state -> [String] in
            let problem = [state.problem?.title, state.problem?.detail].compactMap { $0 }
            let rows = state.holdingGroups.flatMap { group in
                [group.title] + group.rows.flatMap { [$0.name, $0.word, $0.help ?? "", $0.accessibilitySentence] }
            }
            return problem + rows
        }
        for text in texts {
            #expect(text.contains("will sleep") == false)
            #expect(text.contains("sleep now") == false)
            #expect(text.contains("Pause") == false)
            #expect(text.localizedCaseInsensitiveContains("another app") == false)
            #expect(text.localizedCaseInsensitiveContains("other app") == false)
        }
    }
}
