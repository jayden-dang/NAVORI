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
}
