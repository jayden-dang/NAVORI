import AppKit
import Observation
import QuartzCore

/// Owns the menubar item. Left click toggles the panel; right click shows a native menu.
@MainActor
final class StatusBarController: NSObject {
    private let store: CoreStore
    private let openSettings: () -> Void
    private let item: NSStatusItem
    private var panelBuilt = false
    private lazy var panel: StatusPanel = {
        self.panelBuilt = true
        return StatusPanel(store: self.store, openSettings: { [weak self] in self?.showSettings() })
    }()
    private var didScheduleStatusMark = false

    init(store: CoreStore, openSettings: @escaping () -> Void) {
        self.store = store
        self.openSettings = openSettings
        self.item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        super.init()

        if let button = item.button {
            button.target = self
            button.action = #selector(clicked(_:))
            button.sendAction(on: [.leftMouseUp, .rightMouseUp])
            button.toolTip = AppEnvironment.displayName
        }
        item.behavior = .terminationOnRemoval
        observeHealth()
    }

    /// Re-renders the icon whenever `store.health` changes.
    private func observeHealth() {
        withObservationTracking {
            render(health: store.health)
        } onChange: { [weak self] in
            Task { @MainActor in self?.observeHealth() }
        }
    }

    private func render(health: CoreStore.Health) {
        let (symbol, description): (String, String) = switch health {
        case .starting: ("circle.dotted", "Starting")
        case .ok: ("sparkle", "Navori")
        case .degraded: ("exclamationmark.triangle", "Needs attention")
        case .down: ("xmark.octagon", "Core not running")
        }
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: description)
        image?.isTemplate = true
        item.button?.image = image
        item.button?.appearsDisabled = health == .starting
        guard AppEnvironment.perfMode, !didScheduleStatusMark else { return }
        didScheduleStatusMark = true
        CATransaction.begin()
        CATransaction.setCompletionBlock {
            MainActor.assumeIsolated {
                PerfProbe.markStatusItemDrawn()
            }
        }
        CATransaction.commit()
    }

    @objc private func clicked(_ sender: NSStatusBarButton) {
        let isRightClick = NSApp.currentEvent?.type == .rightMouseUp
            || NSApp.currentEvent?.modifierFlags.contains(.control) == true
        if isRightClick {
            showMenu()
        } else if let button = item.button {
            panel.toggle(relativeTo: button)
        }
    }

    func openMenuFresh(trigger: UInt64) {
        let builtBefore = panelBuilt
        if panelBuilt, panel.isVisible {
            let behavior = panel.animationBehavior
            panel.animationBehavior = .none
            panel.close()
            panel.animationBehavior = behavior
        }
        guard let button = item.button else { return }
        panel.show(relativeTo: button) { [weak self] in
            guard let self else { return }
            PerfProbe.markMenuOpened(since: trigger, first: !builtBefore, modules: self.store.moduleCount)
        }
    }

    private func showMenu() {
        panel.close()
        let menu = NSMenu()
        menu.addItem(withTitle: "Open \(AppEnvironment.displayName)", action: #selector(openPanel), keyEquivalent: "").target = self
        menu.addItem(.separator())
        menu.addItem(withTitle: "Settings…", action: #selector(settingsFromMenu), keyEquivalent: ",").target = self
        menu.addItem(withTitle: "Open Logs Folder", action: #selector(openLogs), keyEquivalent: "").target = self
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit \(AppEnvironment.displayName)", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        item.menu = menu
        item.button?.performClick(nil)
        item.menu = nil  // keep left click for the panel
    }

    private func showSettings() {
        panel.close()
        openSettings()
    }

    @objc private func openPanel() {
        if let button = item.button { panel.show(relativeTo: button) }
    }

    @objc private func settingsFromMenu() { showSettings() }

    @objc private func openLogs() {
        guard let dir = store.snapshot?.logsDir else { return }
        NSWorkspace.shared.open(URL(filePath: dir, directoryHint: .isDirectory))
    }
}
