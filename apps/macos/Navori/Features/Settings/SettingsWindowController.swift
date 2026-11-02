import AppKit
import SwiftUI

@MainActor
final class SettingsWindowController {
    private let store: CoreStore
    private let selection = SettingsSelection()
    private var window: NSWindow?

    init(store: CoreStore) {
        self.store = store
    }

    func show(tab: SettingsTab = .general) {
        selection.tab = tab
        if window == nil {
            let hosting = NSHostingController(rootView: SettingsView(store: store, selection: selection))
            let window = NSWindow(contentViewController: hosting)
            window.title = "\(AppEnvironment.displayName) Settings"
            window.styleMask = [.titled, .closable, .miniaturizable]
            window.isReleasedWhenClosed = false
            window.setContentSize(NSSize(width: 560, height: 420))
            window.center()
            self.window = window
        }
        NSApp.activate()
        window?.makeKeyAndOrderFront(nil)
    }
}
