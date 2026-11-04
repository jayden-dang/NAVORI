import NavoriBanner
import NavoriCore
import NavoriPanel
import Testing

@Test func failedStartBannerMessageStartsWithStartupFailed() {
    let error: Error = CoreError.Startup(
        message: "database: sqlite: file is not a database"
    )
    let message = CoreStartupBanner.message(for: error)
    #expect(
        message.hasPrefix("startup failed:"),
        "got \(message)"
    )
}

@Test func a_rejected_awake_action_shows_the_sentence_not_the_type_name() {
    let sentence = "stored leases could not be deleted, so nothing was released"
    let error = CoreError.Dispatch(message: sentence)
    let shown = CoreStartupBanner.menuFailure(for: error)
    let row = AwakeRowState(status: .off, actionFailure: shown)
    let menu = AwakeSubmenuState(
        status: .off,
        leases: [],
        displayHeld: false,
        tracking: true,
        actionFailure: shown
    )
    #expect(row.label == "Needs attention")
    #expect(menu.problem?.title == sentence)
        #expect(menu.problem?.title.contains("CoreError") == false)
}

@Test func a_keychain_failure_shows_the_core_message_unchanged() {
    let error: Error = CoreError.Secrets(message: "keychain is locked")
    #expect(CoreStartupBanner.message(for: error) == "keychain is locked")
}
