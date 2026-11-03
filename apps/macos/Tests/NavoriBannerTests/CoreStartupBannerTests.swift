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
        displayWanted: false,
        actionFailure: shown
    )
    #expect(row.label == sentence)
    #expect(menu.statusDetail == sentence)
    #expect(row.label != String(reflecting: error))
    #expect(menu.statusDetail.contains("CoreError") == false)
}
