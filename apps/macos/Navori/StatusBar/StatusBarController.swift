import AppKit
import NavoriPanel
import Observation
import QuartzCore
import SwiftUI

/// Owns the menubar item. Left click toggles the panel; right click shows a native menu.
@MainActor
final class StatusBarController: NSObject {
    private let store: CoreStore
    private let openSettings: (SettingsTab) -> Void
    private let item: NSStatusItem
    private var panelBuilt = false
    private lazy var panel: StatusPanel = {
        self.panelBuilt = true
        let content = CascadeContent(
            tree: { [weak self] in self?.menuTree() ?? [] },
            root: AnyView(PanelView(store: self.store)),
            submenu: { [weak self] id in
                guard let self, id == "core" else { return AnyView(EmptyView()) }
                return AnyView(CoreSubmenuView(store: self.store))
            },
            activate: { [weak self] id in self?.activate(id) }
        )
        return StatusPanel(content: content, onVisibility: { [weak self] visible in
            self?.store.setPanelVisible(visible)
        })
    }()
    #if DEBUG
    private var previewPanel: StatusPanel?
    #endif
    private var didScheduleStatusMark = false
    private var didReplaceCoreTree = false

    init(store: CoreStore, openSettings: @escaping (SettingsTab) -> Void) {
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
        observeSnapshot()
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
            #if DEBUG
            previewPanel?.close()
            #endif
            panel.toggle(relativeTo: button)
        }
    }

    func openMenuFresh() {
        let builtBefore = panelBuilt
        if panelBuilt, panel.isOpen {
            let behavior = panel.animationBehavior
            panel.animationBehavior = .none
            panel.close()
            panel.animationBehavior = behavior
        }
        guard let button = item.button else {
            PerfProbe.fault("SIGUSR1 open has no status button")
            return
        }
        guard let trigger = PerfProbe.captureTrigger() else { return }
        panel.show(relativeTo: button) { [weak self] in
            guard let self else { return }
            PerfProbe.markMenuOpened(since: trigger, first: !builtBefore, modules: self.store.moduleCount)
        }
    }

    func openCoreSubmenuFresh() {
        guard panelBuilt, panel.isOpen else {
            PerfProbe.fault("SIGUSR2 open has no level 0")
            return
        }
        panel.cascade.closeSubmenus()
        guard let trigger = PerfProbe.captureTrigger() else { return }
        panel.cascade.open("core") {
            PerfProbe.markSubmenuOpened(since: trigger)
        }
    }

    private func showMenu() {
        panel.close()
        #if DEBUG
        previewPanel?.close()
        #endif
        let menu = NSMenu()
        menu.addItem(withTitle: "Open \(AppEnvironment.displayName)", action: #selector(openPanel), keyEquivalent: "").target = self
        #if DEBUG
        menu.addItem(withTitle: "Open Menu Preview", action: #selector(openPreview), keyEquivalent: "").target = self
        #endif
        menu.addItem(.separator())
        menu.addItem(withTitle: "Settings…", action: #selector(settingsFromMenu), keyEquivalent: ",").target = self
        menu.addItem(withTitle: "Open Logs Folder", action: #selector(openLogs), keyEquivalent: "").target = self
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit \(AppEnvironment.displayName)", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        item.menu = menu
        item.button?.performClick(nil)
        item.menu = nil  // keep left click for the panel
    }

    private func observeSnapshot() {
        withObservationTracking {
            _ = store.hasSnapshot
        } onChange: { [weak self] in
            Task { @MainActor in
                self?.onSnapshotChange()
                self?.observeSnapshot()
            }
        }
    }

    private func onSnapshotChange() {
        guard store.hasSnapshot, !didReplaceCoreTree else { return }
        didReplaceCoreTree = true
        guard panelBuilt, panel.isOpen else { return }
        panel.cascade.replaceTree()
    }

    private func menuTree() -> [CascadeNode] {
        let coreChildren: [CascadeNode]
        if store.hasSnapshot {
            let modules = store.moduleRows.map { CascadeNode(id: $0.id) }
            coreChildren = modules.isEmpty ? [CascadeNode(id: "core.modules")] : modules
        } else {
            coreChildren = []
        }
        return [
            CascadeNode(id: "core", isActionable: true, children: coreChildren),
            CascadeNode(id: "settings", isActionable: true),
            CascadeNode(id: "about", isActionable: true),
            CascadeNode(id: "quit", isActionable: true),
        ]
    }

    private func activate(_ id: String) {
        switch id {
        case "settings": showSettings(.general)
        case "about": showSettings(.about)
        case "quit": NSApp.terminate(nil)
        default: break
        }
    }

    private func showSettings(_ tab: SettingsTab = .general) {
        panel.close()
        #if DEBUG
        previewPanel?.close()
        #endif
        openSettings(tab)
    }

    @objc private func openPanel() {
        #if DEBUG
        previewPanel?.close()
        #endif
        if let button = item.button { panel.show(relativeTo: button) }
    }

    #if DEBUG
    @objc private func openPreview() {
        panel.close()
        if previewPanel == nil {
            let created = StatusPanel(
                content: PreviewMenu.content(setTint: { [weak self] tint in
                    self?.previewPanel?.setTint(tint)
                }),
                onVisibility: { _ in }
            )
            previewPanel = created
        }
        if let button = item.button {
            previewPanel?.show(relativeTo: button)
        }
    }
    #endif

    @objc private func settingsFromMenu() { showSettings() }

    @objc private func openLogs() {
        guard let dir = store.snapshot?.logsDir else { return }
        NSWorkspace.shared.open(URL(filePath: dir, directoryHint: .isDirectory))
    }
}
