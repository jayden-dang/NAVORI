import AppKit
import NavoriPanel
import Observation
import os
import QuartzCore
import SwiftUI

@MainActor
@Observable
final class CascadeController {
    private(set) var selection: String?

    @ObservationIgnored private var state: CascadeState
    @ObservationIgnored private var hoverTimer: DispatchWorkItem?
    @ObservationIgnored private var rowFrames = CascadeRowFrames()
    @ObservationIgnored private var panels: [Int: CascadePanel] = [:]
    /// The row whose root view each depth's panel holds. Lets an open skip the rebuild for a row that was
    /// prepared at pointer entry or shown before. Emptied whenever the menu closes or the tree changes.
    @ObservationIgnored private var builtRows: [Int: String] = [:]
    @ObservationIgnored private var lastClickRow: String?
    @ObservationIgnored private var lastClickAt: CFTimeInterval = 0
    @ObservationIgnored private var pendingFirstFrame: (@MainActor () -> Void)?
    @ObservationIgnored private var placing = false
    @ObservationIgnored private var placePending = false
    @ObservationIgnored private let tree: () -> [CascadeNode]
    @ObservationIgnored private let submenu: (String) -> AnyView
    @ObservationIgnored private let activateRow: (String) -> Void
    @ObservationIgnored private let log = Logger(subsystem: "dev.navori", category: "cascade")
    /// Perf harness only: per-step timings of the open path. Nil outside the bench.
    @ObservationIgnored var presentTrace: SubmenuPresentTrace?
    /// Perf harness only: false measures the open without the intent-window preparation.
    @ObservationIgnored var prepareEnabled = true
    @ObservationIgnored var interceptKey: ((CascadeKey) -> Bool)?
    @ObservationIgnored weak var panel: StatusPanel?
    @ObservationIgnored private var surfaceTint: NSColor = GlassTint.standard

    init(
        tree: @escaping () -> [CascadeNode],
        submenu: @escaping (String) -> AnyView,
        activate: @escaping (String) -> Void
    ) {
        self.tree = tree
        self.submenu = submenu
        self.activateRow = activate
        self.state = CascadeState(root: tree())
        self.selection = state.selection
    }

    func pointer(on row: String, at screenPoint: CGPoint, onFirstFrame: (@MainActor () -> Void)? = nil) {
        if let onFirstFrame { pendingFirstFrame = onFirstFrame }
        apply(state.handle(.pointerOn(row: row, at: screenPoint, submenu: openSubmenuRect)))
    }

    func pointerLeft(_ row: String) {
        apply(state.handle(.pointerLeft(row: row)))
    }

    func click(_ row: String) {
        let now = CACurrentMediaTime()
        if lastClickRow == row, now - lastClickAt < 0.25 { return }
        lastClickRow = row
        lastClickAt = now
        apply(state.handle(.click(row: row)))
    }

    func key(_ key: CascadeKey) {
        if interceptKey?(key) == true { return }
        apply(state.handle(.key(key)))
    }

    func setRowFrame(_ row: String, _ frameInPanel: CGRect) {
        rowFrames.record(row, frameInPanel)
        if state.openPath.contains(row) {
            schedulePlace(row)
        }
    }

    func replaceTree() {
        dropBuiltViews(sparingVisible: true)
        apply(state.handle(.replaceTree(tree())))
    }

    func open(_ row: String, onFirstFrame: (@MainActor () -> Void)? = nil) {
        guard panel?.isOpen == true else {
            log.error("submenu open for \(row, privacy: .public) has no panel presented")
            return
        }
        pendingFirstFrame = onFirstFrame
        if state.openPath.contains(row) {
            presentSubmenu(row)
            return
        }
        apply(state.handle(.click(row: row)))
        if pendingFirstFrame != nil {
            log.error("submenu open for \(row, privacy: .public) presented no panel")
            pendingFirstFrame = nil
        }
    }

    func closeSubmenus() {
        hoverTimer?.cancel()
        hoverTimer = nil
        for child in panels.values {
            child.hide()
        }
        dropBuiltViews(sparingVisible: false)
        state = CascadeState(root: tree())
        selection = state.selection
    }

    var keyIsInside: Bool {
        guard let key = NSApp.keyWindow else { return false }
        return owns(key)
    }

    func handleResignKey() {
        if keyIsInside { return }
        panel?.close()
    }

    var openFrames: [CGRect] {
        var frames: [CGRect] = []
        if let panel, panel.isOpen {
            frames.append(panel.frame)
        }
        for (index, _) in state.openPath.enumerated() {
            if let child = panels[index + 1], child.isVisible {
                frames.append(child.frame)
            }
        }
        return frames
    }

    func setTint(_ tint: NSColor) {
        surfaceTint = tint
        for child in panels.values {
            (child.contentView as? GlassSurfaceView)?.tint = tint
        }
    }

    private func apply(_ effects: [CascadeEffect]) {
        for effect in effects {
            switch effect {
            case let .open(row):
                log.info("open: \(row, privacy: .public)")
                presentSubmenu(row)
            case let .prepare(row):
                prepareSubmenu(row)
            case let .close(depth):
                hideFrom(depth: depth)
            case let .armHoverTimer(delay):
                armHoverTimer(after: delay)
            case .cancelHoverTimer:
                hoverTimer?.cancel()
                hoverTimer = nil
            case let .activate(row):
                activateRow(row)
            case .closeAll:
                panel?.close()
            }
        }
        selection = state.selection
    }

    private func presentSubmenu(_ row: String) {
        let depth = (state.openPath.firstIndex(of: row) ?? 0) + 1
        let child = panels[depth] ?? CascadePanel()
        panels[depth] = child
        child.onResignKey = { [weak self] in
            self?.handleResignKey()
        }
        child.onKey = { [weak self] key in
            self?.key(key)
        }
        let t0 = CACurrentMediaTime()
        if builtRows[depth] != row {
            child.setRootView(AnyView(submenu(row).environment(self)))
            builtRows[depth] = row
        }
        let t1 = CACurrentMediaTime()
        child.onFittingSizeChange = { [weak self] in
            self?.schedulePlace(row)
        }
        let firstFrame = pendingFirstFrame
        pendingFirstFrame = nil
        placing = true
        let frame = frameForSubmenu(row, child: child)
        let t2 = CACurrentMediaTime()
        child.show(frame: frame, onFirstFrame: firstFrame)
        (child.contentView as? GlassSurfaceView)?.tint = surfaceTint
        placing = false
        presentTrace?.recordPresent(setRoot: t1 - t0, frame: t2 - t1, show: CACurrentMediaTime() - t2)
        schedulePlace(row)
    }

    /// The invisible half of an open, done while the intent delay runs: build the row's root view in its
    /// depth's panel and measure it. The panel is not ordered front, so nothing appears and nothing takes focus.
    private func prepareSubmenu(_ row: String) {
        guard prepareEnabled, panel?.isOpen == true, let depth = treeDepth(of: row).map({ $0 + 1 }) else { return }
        if let existing = panels[depth], existing.isVisible { return }
        guard builtRows[depth] != row else { return }
        let child = panels[depth] ?? CascadePanel()
        panels[depth] = child
        let started = CACurrentMediaTime()
        child.setRootView(AnyView(submenu(row).environment(self)))
        _ = child.idealSize
        builtRows[depth] = row
        presentTrace?.recordPrepare(CACurrentMediaTime() - started)
    }

    /// Forget every prepared or kept root view, so the next open rebuilds. A panel that is on screen keeps its
    /// content until it is hidden when `sparingVisible`.
    private func dropBuiltViews(sparingVisible: Bool) {
        for depth in Array(builtRows.keys) {
            builtRows[depth] = nil
            guard let child = panels[depth], !(sparingVisible && child.isVisible) else { continue }
            child.setRootView(AnyView(EmptyView()))
        }
    }

    private func schedulePlace(_ row: String) {
        guard !placePending else { return }
        placePending = true
        DispatchQueue.main.async { [weak self] in
            self?.placePending = false
            self?.placeSubmenu(row)
        }
    }

    private func placeSubmenu(_ row: String) {
        guard !placing else { return }
        guard let depth = state.openPath.firstIndex(of: row).map({ $0 + 1 }),
              let child = panels[depth],
              child.isVisible
        else { return }
        placing = true
        defer { placing = false }
        let next = frameForSubmenu(row, child: child)
        guard abs(next.origin.x - child.frame.origin.x) > 0.5
            || abs(next.origin.y - child.frame.origin.y) > 0.5
            || abs(next.width - child.frame.width) > 0.5
            || abs(next.height - child.frame.height) > 0.5
        else { return }
        child.setFrame(next, display: true)
    }

    private func frameForSubmenu(_ row: String, child: CascadePanel) -> CGRect {
        let depth = (state.openPath.firstIndex(of: row) ?? 0) + 1
        var ancestors: [CGRect] = []
        if depth > 1 {
            for level in 1..<depth {
                if let ancestor = panels[level] {
                    ancestors.append(ancestor.frame)
                }
            }
        }
        let levelZero = panel?.frame ?? .zero
        let rowFrame = window(containing: row).flatMap { rowFrames.screenFrame(of: row, in: $0.frame) }
            ?? ancestors.last ?? levelZero
        let visible = panel?.screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? .zero
        return CascadePlacement.submenuFrame(
            levelZero: levelZero,
            ancestorFrames: ancestors,
            row: rowFrame,
            size: child.idealSize,
            visible: visible
        )
    }

    private func owns(_ window: NSWindow) -> Bool {
        if window === panel { return true }
        return panels.values.contains { $0 === window }
    }

    private func hideFrom(depth: Int) {
        for key in panels.keys where key >= depth {
            panels[key]?.hide()
        }
    }

    private var openSubmenuRect: CGRect? {
        guard let last = state.openPath.last,
              let depth = state.openPath.firstIndex(of: last).map({ $0 + 1 }),
              let child = panels[depth],
              child.isVisible
        else { return nil }
        return child.frame
    }

    private func window(containing row: String) -> NSWindow? {
        guard let depth = treeDepth(of: row) else { return panel }
        if depth == 0 { return panel }
        return panels[depth] ?? panel
    }

    private func treeDepth(of row: String) -> Int? {
        func search(_ nodes: [CascadeNode], depth: Int) -> Int? {
            for node in nodes {
                if node.id == row { return depth }
                if let found = search(node.children, depth: depth + 1) {
                    return found
                }
            }
            return nil
        }
        return search(tree(), depth: 0)
    }

    private func armHoverTimer(after delay: TimeInterval) {
        hoverTimer?.cancel()
        let work = DispatchWorkItem { [weak self] in
            MainActor.assumeIsolated {
                self?.hoverTimerFired()
            }
        }
        hoverTimer = work
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }

    private func hoverTimerFired() {
        hoverTimer = nil
        guard let panel, panel.isOpen else { return }
        presentTrace?.timerFiredAt = CACurrentMediaTime()
        apply(state.handle(.hoverTimerFired))
    }
}


/// Perf harness only. The synchronous cost of one submenu open, in seconds.
@MainActor
final class SubmenuPresentTrace {
    var timerFiredAt = 0.0
    private(set) var prepare = 0.0
    private(set) var setRoot = 0.0
    private(set) var frame = 0.0
    private(set) var show = 0.0

    func reset() {
        timerFiredAt = 0
        prepare = 0
        setRoot = 0
        frame = 0
        show = 0
    }

    func recordPrepare(_ seconds: Double) {
        prepare = seconds
    }

    func recordPresent(setRoot: Double, frame: Double, show: Double) {
        self.setRoot = setRoot
        self.frame = frame
        self.show = show
    }

    var presentTotal: Double { setRoot + frame + show }
}
