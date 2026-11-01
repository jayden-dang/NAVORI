import NavoriBanner
import NavoriCore
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
