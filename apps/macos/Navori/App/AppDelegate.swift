import AppKit
import os

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let store = CoreStore()
    private var statusBar: StatusBarController?
    private var wakeObserver: WakeObserver?
    private lazy var settings = SettingsWindowController(store: store)
    private let log = Logger(subsystem: "dev.navori", category: "app")

    func applicationDidFinishLaunching(_ notification: Notification) {
        log.info("launching \(AppEnvironment.displayName, privacy: .public) \(AppEnvironment.version, privacy: .public)")
        EditMenu.install()
        if !AppEnvironment.perfFlagWithoutHome {
            store.start()
            wakeObserver = WakeObserver(
                onWake: { [weak self] in self?.store.systemDidWake() },
                onSleep: { [weak self] in self?.store.systemWillSleep() }
            )
        }
        statusBar = StatusBarController(
            store: store,
            openSettings: { [weak self] tab in self?.settings.show(tab: tab) }
        )
        if let statusBar {
            PerfProbe.start(statusBar: statusBar, store: store)
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        wakeObserver = nil
        store.stop()
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        // Launching the app again (Finder, `open`) opens Settings instead of doing nothing.
        settings.show()
        return false
    }
}
