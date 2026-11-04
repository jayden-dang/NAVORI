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
        tracking: Bool = true,
        actionFailure: String? = nil
    ) -> AwakeSubmenuState {
        AwakeSubmenuState(
            status: status,
            leases: leases,
            displayHeld: displayHeld,
            tracking: tracking,
            actionFailure: actionFailure
        )
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

    private static func length(_ seconds: Int) -> KeepAwakeChoice {
        guard let choice = KeepAwakeChoice.all.first(where: { $0.seconds == seconds }) else {
            preconditionFailure("no menu length of \(seconds) seconds")
        }
        return choice
    }

    @Test func keep_awake_offers_indefinitely_then_each_length_in_order() {
        #expect(KeepAwakeChoice.all.map(\.title) == [
            "Indefinitely", "5 minutes", "10 minutes", "15 minutes",
            "30 minutes", "1 hour", "2 hours", "5 hours",
        ])
        #expect(KeepAwakeChoice.all.map(\.seconds) == [nil, 300, 600, 900, 1800, 3600, 7200, 18000])
    }

    @Test func a_choice_is_found_again_by_its_row_id() {
        for choice in KeepAwakeChoice.all {
            #expect(KeepAwakeChoice(id: choice.id) == choice)
        }
        #expect(KeepAwakeChoice(id: "awake.tracking") == nil)
        #expect(KeepAwakeChoice(id: "awake.keep") == nil)
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

    private static func titled(_ title: String?, label: String = "klynt/dev", pid: Int32 = 51865) -> AwakeLeaseInput {
        AwakeLeaseInput(
            kind: .session, harness: "Claude Code", label: label, pid: pid, stopped: false, until: nil, title: title
        )
    }

    @Test func a_session_row_leads_with_its_title_and_keeps_its_project_group() {
        let state = Self.state(leases: [Self.titled("Road-128 ticket shaping")])
        #expect(state.holdingGroups.map(\.title) == ["klynt"])
        let row = state.holdingGroups[0].rows[0]
        #expect(row.name == "Road-128 ticket shaping")
        #expect(row.help == "Road-128 ticket shaping · Claude Code · klynt/dev · pid 51865")
        #expect(row.accessibilitySentence == "Road-128 ticket shaping, Claude Code, klynt/dev, working, pid 51865")
    }

    @Test func a_session_with_no_title_still_reads_its_branch() {
        for title in [nil, ""] as [String?] {
            let row = Self.state(leases: [Self.titled(title)]).holdingGroups[0].rows[0]
            #expect(row.name == "dev")
            #expect(row.help == "Claude Code · klynt/dev · pid 51865")
            #expect(row.accessibilitySentence == "Claude Code, klynt/dev, working, pid 51865")
        }
    }

    @Test func sessions_in_a_project_sort_by_the_name_they_show() {
        let state = Self.state(leases: [
            Self.titled("Zebra", pid: 1),
            Self.titled("Alpha", pid: 2),
        ])
        #expect(state.holdingGroups[0].rows.map(\.name) == ["Alpha", "Zebra"])
    }

    @Test func nothing_is_checked_and_the_row_has_no_value_while_no_keep_awake_lease_exists() {
        let off = Self.state(status: .off)
        #expect(off.keepValue == nil)
        #expect(KeepAwakeChoice.all.allSatisfy { off.isChecked($0) == false })

        let onlySession = Self.state(leases: [Self.session("Codex", "a/b", pid: 1), Self.command])
        #expect(onlySession.keepValue == nil)
        #expect(KeepAwakeChoice.all.allSatisfy { onlySession.isChecked($0) == false })
    }

    @Test func a_duration_lease_checks_its_length_and_shows_its_end_time() {
        let hour = Self.state(leases: [Self.hour])
        #expect(hour.keepValue == "until 11:38")
        #expect(hour.isChecked(Self.length(3600)))
        #expect(hour.isChecked(Self.length(300)) == false)
        #expect(hour.isChecked(.indefinitely) == false)
    }

    @Test func a_display_or_until_off_lease_checks_indefinitely() {
        for lease in [Self.display, Self.untilOff] {
            let state = Self.state(leases: [lease], displayHeld: true)
            #expect(state.keepValue == "Indefinitely")
            #expect(state.isChecked(.indefinitely))
            #expect(state.isChecked(Self.length(3600)) == false)
        }
    }

    @Test func a_length_the_menu_does_not_offer_checks_nothing_but_still_shows_its_end_time() {
        let ninety = AwakeLeaseInput(
            kind: .duration, harness: nil, label: "90 minutes", pid: nil, stopped: false, until: "12:08"
        )
        let state = Self.state(leases: [ninety])
        #expect(state.keepValue == "until 12:08")
        #expect(KeepAwakeChoice.all.allSatisfy { state.isChecked($0) == false })
    }

    @Test func picking_an_unchecked_choice_asks_for_it() {
        let state = Self.state(leases: [Self.hour])
        #expect(state.command(picking: Self.length(300)) == .keepFor(seconds: 300))
        #expect(state.command(picking: .indefinitely) == .keepIndefinitely)
    }

    @Test func picking_the_checked_choice_turns_keep_awake_off() {
        #expect(Self.state(leases: [Self.hour]).command(picking: Self.length(3600)) == .keepOff)
        #expect(Self.state(leases: [Self.display]).command(picking: .indefinitely) == .keepOff)
        #expect(Self.state(leases: [Self.untilOff]).command(picking: .indefinitely) == .keepOff)
    }

    @Test func tracking_agents_reads_on_with_a_green_dot_or_off_with_none() {
        let on = Self.state(status: .off, tracking: true)
        #expect(on.trackingOn)
        #expect(on.trackingValue == "On")
        #expect(on.trackingDot == .green)

        let off = Self.state(status: .off, tracking: false)
        #expect(off.trackingOn == false)
        #expect(off.trackingValue == "Off")
        #expect(off.trackingDot == nil)
    }

    @Test func commands_become_the_actions_the_module_reads() {
        #expect(AwakeCommand.keepFor(seconds: 300).actionJSON == "{\"keep_for\":{\"seconds\":300}}")
        #expect(AwakeCommand.keepIndefinitely.actionJSON == "\"keep_indefinitely\"")
        #expect(AwakeCommand.keepOff.actionJSON == "\"keep_off\"")
        #expect(AwakeCommand.tracking(false).actionJSON == "{\"tracking\":{\"on\":false}}")
        #expect(AwakeCommand.tracking(true).actionJSON == "{\"tracking\":{\"on\":true}}")
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
