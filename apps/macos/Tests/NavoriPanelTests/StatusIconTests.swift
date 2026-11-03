import NavoriPanel
import Testing

@Suite
struct StatusIconTests {
    private let core = StatusIconState(glyph: .lease, attention: true, description: "Awake held")

    @Test func a_failed_core_is_core_down_whatever_the_core_said() {
        let state = StatusIconState(coreFailed: true, core: core)
        #expect(state.glyph == .coreDown)
        #expect(state.description == "Core not running")
        #expect(StatusIconState(coreFailed: true, core: nil).glyph == .coreDown)
    }

    @Test func no_core_state_yet_is_starting() {
        let state = StatusIconState(coreFailed: false, core: nil)
        #expect(state.glyph == .starting)
        #expect(state.description == "Starting")
    }

    @Test func a_core_state_is_used_unchanged() {
        #expect(StatusIconState(coreFailed: false, core: core) == core)
    }

    private func state(_ glyph: StatusIconGlyph, attention: Bool = false) -> StatusIconState {
        StatusIconState(glyph: glyph, attention: attention, description: "")
    }

    private func plan(
        _ motion: inout StatusIconMotion, _ state: StatusIconState,
        reduceMotion: Bool = false, allowBeat: Bool = true
    ) -> StatusIconMotion.Plan {
        motion.next(state, reduceMotion: reduceMotion, allowBeat: allowBeat)
    }

    @Test(arguments: [StatusIconGlyph.normal, .lease, .problem, .coreDown, .starting])
    func the_loop_plays_only_on_the_ai_glyph(glyph: StatusIconGlyph) {
        var motion = StatusIconMotion()
        #expect(plan(&motion, state(glyph)).loop == false)
        #expect(plan(&motion, state(.aiRunning)).loop)
    }

    @Test func attention_turning_on_plays_one_beat() {
        var motion = StatusIconMotion()
        _ = plan(&motion, state(.normal))
        #expect(plan(&motion, state(.normal, attention: true)).beat)
    }

    @Test func the_first_snapshot_with_attention_on_plays_no_beat() {
        var motion = StatusIconMotion()
        #expect(plan(&motion, state(.normal, attention: true)).beat == false)
    }

    @Test(arguments: [StatusIconGlyph.starting, .coreDown])
    func no_beat_when_the_previous_state_was_starting_or_core_down(previous: StatusIconGlyph) {
        var motion = StatusIconMotion()
        _ = plan(&motion, state(previous))
        #expect(plan(&motion, state(.normal, attention: true)).beat == false)
    }

    @Test func attention_staying_on_plays_no_more_beats() {
        var motion = StatusIconMotion()
        _ = plan(&motion, state(.normal))
        _ = plan(&motion, state(.normal, attention: true))
        #expect(plan(&motion, state(.normal, attention: true)).beat == false)
        #expect(plan(&motion, state(.lease, attention: true)).beat == false)
    }

    @Test func attention_turning_on_again_after_off_plays_a_beat() {
        var motion = StatusIconMotion()
        _ = plan(&motion, state(.normal))
        _ = plan(&motion, state(.normal, attention: true))
        _ = plan(&motion, state(.normal))
        #expect(plan(&motion, state(.normal, attention: true)).beat)
    }

    @Test func reduce_motion_plays_no_loop_and_no_beat() {
        var motion = StatusIconMotion()
        _ = plan(&motion, state(.aiRunning), reduceMotion: true)
        let result = plan(&motion, state(.aiRunning, attention: true), reduceMotion: true)
        #expect(result == StatusIconMotion.Plan(loop: false, beat: false))
    }

    @Test func a_beat_during_the_ai_loop_keeps_the_loop_in_the_plan() {
        var motion = StatusIconMotion()
        _ = plan(&motion, state(.aiRunning))
        let result = plan(&motion, state(.aiRunning, attention: true))
        #expect(result == StatusIconMotion.Plan(loop: true, beat: true))
    }

    @Test func allow_beat_false_plays_no_beat() {
        var motion = StatusIconMotion()
        _ = plan(&motion, state(.normal))
        #expect(plan(&motion, state(.normal, attention: true), allowBeat: false).beat == false)
        // the state was still recorded, so staying on does not beat later
        #expect(plan(&motion, state(.normal, attention: true)).beat == false)
    }
}
