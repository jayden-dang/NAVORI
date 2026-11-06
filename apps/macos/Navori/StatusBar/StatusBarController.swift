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
                guard let self else { return AnyView(EmptyView()) }
                switch id {
                case "core":
                    return AnyView(CoreSubmenuView(store: self.store))
                case "running":
                    return AnyView(RunningSubmenuView(store: self.store))
                case "awake.keep":
                    return AnyView(KeepAwakeSubmenuView(store: self.store))
                case _ where id.hasPrefix("running.project.") || id == "running.other":
                    return AnyView(ProjectChildView(projectId: id, store: self.store))
                default:
                    return AnyView(EmptyView())
                }
            },
            activate: { [weak self] id in self?.activate(id) }
        )
        let panel = StatusPanel(content: content, onVisibility: { [weak self] visible in
            self?.store.setPanelVisible(visible)
        })
        panel.cascade.interceptKey = { [weak self] key in
            guard let store = self?.store, store.isAskingToStop else { return false }
            switch key {
            case .escape:
                store.dismissAskingStops()
                return true
            case .return:
                return true
            case .up, .down, .left, .right:
                return false
            }
        }
        return panel
    }()
    #if DEBUG
    private var previewPanel: StatusPanel?
    private var iconSample: StatusIconState?
    #endif
    private var didScheduleStatusMark = false
    private var didReplaceCoreTree = false
    private var runningHadSubmenu = false
    private var runningTree: [CascadeNode] = []
    private var coreRowWasVisible = false
    private var motion = StatusIconMotion()
    private var drawn: (state: StatusIconState, loop: Bool)?
    private var accessibilityObserver: NSObjectProtocol?

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
        runningHadSubmenu = RunningRowState(running: store.running).hasSubmenu
        runningTree = store.runningTree
        accessibilityObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification,
            object: NSWorkspace.shared,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.render(self.store.statusIcon)
            }
        }
        observeStatusIcon()
        observeSnapshot()
    }

    isolated deinit {
        if let accessibilityObserver {
            NSWorkspace.shared.notificationCenter.removeObserver(accessibilityObserver)
        }
    }

    /// Re-renders the icon whenever `store.statusIcon` changes.
    private func observeStatusIcon() {
        withObservationTracking {
            render(store.statusIcon)
        } onChange: { [weak self] in
            Task { @MainActor in self?.observeStatusIcon() }
        }
    }

    private func render(_ real: StatusIconState, allowBeat: Bool = true) {
        #if DEBUG
        let state = iconSample ?? real
        let beatAllowed = allowBeat && iconSample == nil
        #else
        let state = real
        let beatAllowed = allowBeat
        #endif
        let plan = motion.next(
            state,
            reduceMotion: NSWorkspace.shared.accessibilityDisplayShouldReduceMotion,
            allowBeat: beatAllowed
        )
        if let drawn, drawn.state == state, drawn.loop == plan.loop { return }
        drawn = (state, plan.loop)
        item.button?.image = StatusIconImage.image(state)
        item.button?.appearsDisabled = state.glyph == .starting
        applyMotion(plan)
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

    /// Opacity motion runs in Core Animation, off the main thread, with no timer.
    private func applyMotion(_ plan: StatusIconMotion.Plan) {
        guard let button = item.button else { return }
        if (plan.loop || plan.beat) && !button.wantsLayer { button.wantsLayer = true }
        guard let layer = button.layer else { return }
        if plan.loop {
            if layer.animation(forKey: "icon.loop") == nil {
                let loop = CABasicAnimation(keyPath: "opacity")
                loop.fromValue = 1.0
                loop.toValue = 0.35
                loop.duration = 1
                loop.autoreverses = true
                loop.repeatCount = .infinity
                // A frame-rate hint keeps the render server from redrawing at the display rate for
                // the whole pulse; 10 fps still reads as one smooth pulse.
                loop.preferredFrameRateRange = CAFrameRateRange(minimum: 8, maximum: 12, preferred: 10)
                layer.add(loop, forKey: "icon.loop")
            }
        } else {
            layer.removeAnimation(forKey: "icon.loop")
        }
        if plan.beat { addBeat(to: layer) }
    }

    private func addBeat(to layer: CALayer) {
        let beat = CABasicAnimation(keyPath: "opacity")
        beat.fromValue = 1.0
        beat.toValue = 0.2
        beat.duration = 0.25
        beat.autoreverses = true
        layer.add(beat, forKey: "icon.beat")
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
        guard store.coreRowVisible else {
            PerfProbe.fault("SIGUSR2 open: the Core row is hidden")
            return
        }
        panel.cascade.closeSubmenus()
        guard let trigger = PerfProbe.captureTrigger() else { return }
        panel.cascade.open("core") {
            PerfProbe.markSubmenuOpened(since: trigger)
        }
    }

    /// Perf mode only (SIGINFO): the pointer enters the Core row, the real intent timer fires, the Submenu opens.
    func openCoreSubmenuByHoverFresh() {
        guard panelBuilt, panel.isOpen else {
            PerfProbe.fault("SIGINFO open has no level 0")
            return
        }
        panel.cascade.closeSubmenus()
        guard let trigger = PerfProbe.captureTrigger() else { return }
        panel.cascade.pointer(on: "core", at: CGPoint(x: 100, y: 200)) {
            PerfProbe.markSubmenuIntentOpened(since: trigger)
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
            _ = store.runningTree
            _ = store.coreRowVisible
        } onChange: { [weak self] in
            Task { @MainActor in
                self?.onSnapshotChange()
                self?.observeSnapshot()
            }
        }
    }

    private func onSnapshotChange() {
        let hasSubmenu = RunningRowState(running: store.running).hasSubmenu
        let tree = store.runningTree
        let coreArrived = store.hasSnapshot && !didReplaceCoreTree
        let coreRowVisible = store.coreRowVisible
        let changed = hasSubmenu != runningHadSubmenu || tree != runningTree || coreRowVisible != coreRowWasVisible
        if coreArrived {
            didReplaceCoreTree = true
        }
        runningHadSubmenu = hasSubmenu
        runningTree = tree
        coreRowWasVisible = coreRowVisible
        guard panelBuilt, panel.isOpen, coreArrived || changed else { return }
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
        let runningChildren = store.hasSnapshot && RunningRowState(running: store.running).hasSubmenu
            ? runningCommands + store.runningTree
            : []
        return LevelZeroTree.rows(
            running: CascadeNode(id: "running", isActionable: true, children: runningChildren),
            core: CascadeNode(id: "core", isActionable: true, children: coreChildren),
            status: store.coreStatus,
            inDevProfile: AppEnvironment.isDev
        )
    }

    private var runningCommands: [CascadeNode] {
        [
            CascadeNode(
                id: "awake.keep",
                isActionable: true,
                children: KeepAwakeChoice.all.map { CascadeNode(id: $0.id, isActionable: true) }
            ),
            CascadeNode(id: "awake.tracking", isActionable: true),
            CascadeNode(id: "awake.lid", isActionable: true),
        ]
    }

    private func activate(_ id: String) {
        switch id {
        case CoreStore.stopAllId:
            if let all = store.runningSubmenu?.stopAll { store.askStop(id, count: all.ports) }
        case _ where id.hasPrefix("running."): store.askChildStop(id)
        case "settings": showSettings(.general)
        case "about": showSettings(.about)
        case "quit": NSApp.terminate(nil)
        case "awake.tracking":
            let command = AwakeCommand.tracking(store.awakeSubmenu.trackingOn == false)
            Task { [store] in await store.perform(command) }
        case "awake.lid":
            let command = store.awakeSubmenu.lidToggle
            Task { [store] in await store.perform(command) }
        default:
            guard let choice = KeepAwakeChoice(id: id) else { return }
            let command = store.awakeSubmenu.command(picking: choice)
            Task { [store] in await store.perform(command) }
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
    /// Holds a Sample on the Icon until cleared; the real snapshot returns with no Beat.
    func setIconSample(_ sample: StatusIconState?) {
        iconSample = sample
        render(store.statusIcon, allowBeat: false)
    }

    /// Plays one Beat on whatever the Icon shows.
    func playIconBeat() {
        guard !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion,
              let button = item.button else { return }
        if !button.wantsLayer { button.wantsLayer = true }
        if let layer = button.layer { addBeat(to: layer) }
    }

    @objc private func openPreview() {
        panel.close()
        if previewPanel == nil {
            let created = StatusPanel(
                content: PreviewMenu.content(
                    setTint: { [weak self] tint in
                        self?.previewPanel?.setTint(tint)
                    },
                    setIconSample: { [weak self] sample in self?.setIconSample(sample) },
                    playBeat: { [weak self] in self?.playIconBeat() }
                ),
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
