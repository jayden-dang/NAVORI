import AppKit
import NavoriPanel
import Observation
import os
import QuartzCore

@MainActor
@Observable
final class CascadeController {
    private(set) var selection: String?

    @ObservationIgnored private var state: CascadeState
    @ObservationIgnored private var hoverTimer: DispatchWorkItem?
    @ObservationIgnored private var rowFrames: [String: CGRect] = [:]
    @ObservationIgnored private var lastClickRow: String?
    @ObservationIgnored private var lastClickAt: CFTimeInterval = 0
    @ObservationIgnored private let tree: () -> [CascadeNode]
    @ObservationIgnored private let activateRow: (String) -> Void
    @ObservationIgnored private let log = Logger(subsystem: "dev.navori", category: "cascade")
    @ObservationIgnored weak var panel: StatusPanel?

    private static let hoverDelay: TimeInterval = 0.150

    init(tree: @escaping () -> [CascadeNode], activate: @escaping (String) -> Void) {
        self.tree = tree
        self.activateRow = activate
        self.state = CascadeState(root: tree())
        self.selection = state.selection
    }

    func pointer(on row: String, at screenPoint: CGPoint) {
        apply(state.handle(.pointerOn(row: row, at: screenPoint, submenu: nil)))
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
        if let panel {
            rowFrames[row] = panel.convertToScreen(frameInPanel)
        } else {
            rowFrames[row] = frameInPanel
        }
    }

    func reset() {
        hoverTimer?.cancel()
        hoverTimer = nil
        state = CascadeState(root: tree())
        selection = state.selection
        rowFrames.removeAll()
    }

    private func apply(_ effects: [CascadeEffect]) {
        for effect in effects {
            switch effect {
            case let .open(row):
                log.info("open ignored: \(row, privacy: .public)")
            case .close:
                break
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
