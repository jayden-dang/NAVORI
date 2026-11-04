import NavoriPanel
import Testing

@Suite
struct AwakeRowStateTests {
    @Test func each_status_has_its_label_dot_and_submenu() {
        let cases: [(AwakeStatus, String, StatusDot?, Bool)] = [
            (.off, "Off", nil, true),
            (.on(sources: ["Claude Code", "1 hour"]), "On · Claude Code, 1 hour", .green, true),
            (.notHeld, "Not held", nil, true),
            (.unavailable, "Unavailable", nil, false),
            (.problem("Awake could not save a lease. Navori retries in 60 seconds.", held: true),
             "Needs attention", .green, true),
            (.problem("Awake could not save a lease. Navori retries in 60 seconds.", held: false),
             "Needs attention", nil, true),
        ]
        for (status, label, dot, hasSubmenu) in cases {
            let row = AwakeRowState(status: status)
            #expect(row.label == label)
            #expect(row.dot == dot)
            #expect(row.hasSubmenu == hasSubmenu)
        }
    }

    @Test func a_source_holding_several_leases_is_named_once_with_its_count() {
        let row = AwakeRowState(status: .on(sources: [
            "Claude Code", "Codex", "Claude Code", "1 hour", "Claude Code", "Codex",
        ]))
        #expect(row.label == "On · Claude Code ×3, Codex ×2, 1 hour")
    }

    @Test func a_problem_with_no_lease_is_not_held() {
        let status = AwakeLevel0.row(
            problem: "Awake could not read its leases.",
            systemHeld: false,
            hasLease: false,
            sources: []
        )
        #expect(status == .problem("Awake could not read its leases.", held: false))
        #expect(AwakeRowState(status: status).dot == nil)
    }

    @Test func a_stopped_module_stays_unavailable() {
        let status = AwakeLevel0.row(
            problem: AwakeLevel0.moduleStopped,
            systemHeld: false,
            hasLease: true,
            sources: ["1 hour"]
        )
        #expect(status == .unavailable)
        #expect(AwakeRowState(status: status).label == "Unavailable")
    }

    @Test func a_lease_without_the_system_assertion_stays_not_held() {
        let refused = AwakeLevel0.row(
            problem: "macOS refused the request. Navori keeps trying.",
            systemHeld: false,
            hasLease: true,
            sources: ["1 hour"]
        )
        #expect(refused == .notHeld)
        #expect(AwakeRowState(status: refused).label == "Not held")

        let bare = AwakeLevel0.row(
            problem: nil,
            systemHeld: false,
            hasLease: true,
            sources: ["1 hour"]
        )
        #expect(bare == .notHeld)
    }

    @Test func another_problem_does_not_read_as_on_while_the_assertion_is_held() {
        let problem = "Awake could not save a lease. Navori retries in 60 seconds."
        let status = AwakeLevel0.row(
            problem: problem,
            systemHeld: true,
            hasLease: true,
            sources: ["1 hour"]
        )
        #expect(status == .problem(problem, held: true))
        let row = AwakeRowState(status: status)
        #expect(row.label == "Needs attention")
        #expect(row.dot == .green)
    }

    @Test func a_held_lease_with_no_problem_names_its_sources() {
        let status = AwakeLevel0.row(
            problem: nil,
            systemHeld: true,
            hasLease: true,
            sources: ["30 minutes"]
        )
        #expect(status == .on(sources: ["30 minutes"]))
        #expect(AwakeRowState(status: status).label == "On · 30 minutes")
    }

    @Test func no_lease_and_no_problem_reads_off() {
        let status = AwakeLevel0.row(
            problem: nil,
            systemHeld: false,
            hasLease: false,
            sources: []
        )
        #expect(status == .off)
    }

    @Test func a_failed_menu_action_stays_on_the_row_until_the_snapshot_achieves_it() {
        let marks = AwakeHeldMarks(
            durationEndsAtMs: nil,
            hasUntilOff: false,
            hasDisplay: false,
            hasLease: false
        )
        let failure = AwakeActionFailure(
            request: .untilOff,
            message: "The Navori core is not running.",
            marks: marks
        )
        let shown = failure.visible(against: marks)
        #expect(shown == "The Navori core is not running.")
        let row = AwakeRowState(status: .off, actionFailure: shown)
        #expect(row.label == "Needs attention")
        #expect(row.dot == nil)
        #expect(AwakeRowState(status: .on(sources: ["1 hour"]), actionFailure: shown).dot == .green)

        let achieved = AwakeHeldMarks(
            durationEndsAtMs: nil,
            hasUntilOff: true,
            hasDisplay: false,
            hasLease: true
        )
        #expect(failure.visible(against: achieved) == nil)
    }

    @Test func a_failed_duration_request_stays_until_the_deadline_changes() {
        let before = AwakeHeldMarks(
            durationEndsAtMs: 1_700_000_000_000,
            hasUntilOff: false,
            hasDisplay: false,
            hasLease: true
        )
        let failure = AwakeActionFailure(
            request: .duration,
            message: "duration must be from 1 second through 7 days, not 0 seconds",
            marks: before
        )
        #expect(failure.visible(against: before) == failure.message)
        let sameMinute = AwakeHeldMarks(
            durationEndsAtMs: 1_700_003_600_000,
            hasUntilOff: false,
            hasDisplay: false,
            hasLease: true
        )
        #expect(failure.visible(against: sameMinute) == nil)
    }

    @Test func release_all_failure_stays_while_a_lease_remains() {
        let before = AwakeHeldMarks(
            durationEndsAtMs: 11,
            hasUntilOff: false,
            hasDisplay: false,
            hasLease: true
        )
        let failure = AwakeActionFailure(
            request: .releaseAll,
            message: "stored leases could not be deleted, so nothing was released",
            marks: before
        )
        #expect(failure.visible(against: before) == failure.message)
        let cleared = AwakeHeldMarks(
            durationEndsAtMs: nil,
            hasUntilOff: false,
            hasDisplay: false,
            hasLease: false
        )
        #expect(failure.visible(against: cleared) == nil)
    }

    @Test func a_display_failure_clears_only_when_the_display_lease_matches_the_request() {
        let off = AwakeHeldMarks(
            durationEndsAtMs: nil,
            hasUntilOff: false,
            hasDisplay: false,
            hasLease: false
        )
        let on = AwakeHeldMarks(
            durationEndsAtMs: nil,
            hasUntilOff: false,
            hasDisplay: true,
            hasLease: true
        )
        let turnOn = AwakeActionFailure(
            request: .display(true),
            message: "macOS refused to keep the display on. Navori keeps trying.",
            marks: off
        )
        #expect(turnOn.visible(against: off) == turnOn.message)
        #expect(turnOn.visible(against: on) == nil)

        let turnOff = AwakeActionFailure(
            request: .display(false),
            message: "The Navori core is not running.",
            marks: on
        )
        #expect(turnOff.visible(against: on) == turnOff.message)
        #expect(turnOff.visible(against: off) == nil)
    }
}
