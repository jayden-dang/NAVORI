import AppKit
import os

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let store = CoreStore()
    private var statusBar: StatusBarController?
    private lazy var settings = SettingsWindowController(store: store)
    private let log = Logger(subsystem: "dev.navori", category: "app")

    func applicationDidFinishLaunching(_ notification: Notification) {
        log.info("launching \(AppEnvironment.displayName, privacy: .public) \(AppEnvironment.version, privacy: .public)")
        store.start()
        statusBar = StatusBarController(
            store: store,
            openSettings: { [weak self] in self?.settings.show() }
        )
    }

    func applicationWillTerminate(_ notification: Notification) {
        store.stop()
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        // Launching the app again (Finder, `open`) opens Settings instead of doing nothing.
        settings.show()
        return false
    }
}
