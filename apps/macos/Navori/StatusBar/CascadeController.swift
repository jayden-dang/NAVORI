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
    @ObservationIgnored private var lastClickRow: String?
    @ObservationIgnored private var lastClickAt: CFTimeInterval = 0
    @ObservationIgnored private var pendingFirstFrame: (@MainActor () -> Void)?
    @ObservationIgnored private var placing = false
    @ObservationIgnored private var placePending = false
    @ObservationIgnored private let tree: () -> [CascadeNode]
    @ObservationIgnored private let submenu: (String) -> AnyView
    @ObservationIgnored private let activateRow: (String) -> Void
    @ObservationIgnored private let log = Logger(subsystem: "dev.navori", category: "cascade")
    @ObservationIgnored weak var panel: StatusPanel?
    @ObservationIgnored private var surfaceTint: NSColor = GlassTint.standard

    private static let hoverDelay: TimeInterval = 0.150

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

    func pointer(on row: String, at screenPoint: CGPoint) {
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
        apply(state.handle(.key(key)))
    }

    func setRowFrame(_ row: String, _ frameInPanel: CGRect) {
        rowFrames.record(row, frameInPanel)
        if state.openPath.contains(row) {
            schedulePlace(row)
        }
    }

    func replaceTree() {
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
            case let .close(depth):
                hideFrom(depth: depth)
            case .armHoverTimer:
                armHoverTimer()
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
        child.setRootView(AnyView(submenu(row).environment(self)))
        child.onFittingSizeChange = { [weak self] in
            self?.schedulePlace(row)
        }
        let firstFrame = pendingFirstFrame
        pendingFirstFrame = nil
        placing = true
        child.show(frame: frameForSubmenu(row, child: child), onFirstFrame: firstFrame)
        (child.contentView as? GlassSurfaceView)?.tint = surfaceTint
        placing = false
        schedulePlace(row)
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

    private func armHoverTimer() {
        hoverTimer?.cancel()
        let work = DispatchWorkItem { [weak self] in
            MainActor.assumeIsolated {
                self?.hoverTimerFired()
            }
        }
        hoverTimer = work
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.hoverDelay, execute: work)
    }

    private func hoverTimerFired() {
        hoverTimer = nil
        guard let panel, panel.isOpen else { return }
        apply(state.handle(.hoverTimerFired))
    }
}
