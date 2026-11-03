import NavoriPanel
import Testing

@Suite
struct ActionErrorLineTests {
    @Test func a_failed_action_stays_visible_after_the_status_refresh_that_follows_it() {
        var line = ActionErrorLine()
        line.actionFailed("Could not remove Navori's hooks for Grok Build.")
        line.statusLoaded()
        #expect(line.message == "Could not remove Navori's hooks for Grok Build.")
    }

    @Test func the_next_action_that_succeeds_clears_it() {
        var line = ActionErrorLine()
        line.actionFailed("Could not remove Navori's hooks for Grok Build.")
        line.statusLoaded()
        line.actionSucceeded()
        #expect(line.message == nil)
    }

    @Test func a_status_failure_clears_on_the_next_status_read_that_works() {
        var line = ActionErrorLine()
        line.statusFailed("The Navori core is not running.")
        #expect(line.message == "The Navori core is not running.")
        line.statusLoaded()
        #expect(line.message == nil)
    }
}
