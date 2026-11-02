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
    @ObservationIgnored private var rowFrames: [String: CGRect] = [:]
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
        rowFrames[row] = screenFrame(fromSwiftUI: frameInPanel)
        if state.openPath.contains(row) {
            schedulePlace(row)
        }
    }

    func replaceTree() {
        apply(state.handle(.replaceTree(tree())))
    }

    func open(_ row: String, onFirstFrame: (@MainActor () -> Void)? = nil) {
        pendingFirstFrame = onFirstFrame
        if state.openPath.contains(row) {
            presentSubmenu(row)
            return
        }
        apply(state.handle(.click(row: row)))
        if pendingFirstFrame != nil {
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

    func reset() {
        closeSubmenus()
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
        child.setRootView(AnyView(submenu(row).environment(self)))
        child.onFittingSizeChange = { [weak self] in
            self?.schedulePlace(row)
        }
        let firstFrame = pendingFirstFrame
        pendingFirstFrame = nil
        placing = true
        child.show(frame: frameForSubmenu(row, child: child), onFirstFrame: firstFrame)
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
        let parentFrame = panel?.frame ?? .zero
        let rowFrame = rowFrames[row] ?? parentFrame
        let visible = panel?.screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? .zero
        return CascadePlacement.frame(
            parent: parentFrame,
            row: rowFrame,
            size: child.idealSize,
            visible: visible
        )
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

    private func screenFrame(fromSwiftUI rect: CGRect) -> CGRect {
        guard let panel else { return rect }
        let windowRect = CGRect(
            x: rect.origin.x,
            y: panel.frame.height - rect.origin.y - rect.height,
            width: rect.width,
            height: rect.height
        )
        return panel.convertToScreen(windowRect)
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
        guard let panel, panel.isVisible else { return }
        apply(state.handle(.hoverTimerFired))
    }
}
