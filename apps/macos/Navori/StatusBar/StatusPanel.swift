import AppKit
import NavoriPanel
import QuartzCore
import SwiftUI

/// Borderless, non-activating panel anchored under the menubar item — the Navori "popover".
/// Closes on Escape, on click outside, and when it loses key status.
@MainActor
final class StatusPanel: NSPanel {
    let cascade: CascadeController
    private let content: CascadeContent
    private let onVisibility: (Bool) -> Void
    private var outsideClickMonitor: Any?
    private var frameLink: CADisplayLink?
    private var frameCallback: (@MainActor () -> Void)?
    private var refitPending = false
    /// Exact-once flag. `super.close()` can re-enter through `resignKey`.
    private(set) var isOpen = false
    /// Extra click can resign key (async close) then fire `toggle` and reopen.
    private var closedAt: CFTimeInterval = 0
    private static let width: CGFloat = 360
    private static let maxHeight: CGFloat = 560

    init(content: CascadeContent, onVisibility: @escaping (Bool) -> Void) {
        self.content = content
        self.onVisibility = onVisibility
        self.cascade = CascadeController(
            tree: content.tree,
            submenu: content.submenu,
            activate: content.activate
        )
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: Self.width, height: 420),
            styleMask: [.nonactivatingPanel, .borderless, .fullSizeContentView],
            backing: .buffered,
            defer: true
        )
        cascade.panel = self
        isFloatingPanel = true
        level = .popUpMenu
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        hidesOnDeactivate = false
        isMovable = false
        acceptsMouseMovedEvents = true
        backgroundColor = NSColor.black.withAlphaComponent(0.01)
        isOpaque = false
        hasShadow = true
        animationBehavior = .utilityWindow

        let hosting = MenuHostingView(rootView: self.content.root.environment(self.cascade))
        hosting.sizingOptions = [.intrinsicContentSize]
        hosting.onFittingSizeChange = { [weak self] in
            self?.scheduleRefit()
        }
        contentView = GlassSurfaceView(content: hosting)
    }

    override var canBecomeKey: Bool { true }

    func toggle(relativeTo button: NSStatusBarButton) {
        if isOpen {
            close()
        } else if CACurrentMediaTime() - closedAt < 0.25 {
            return
        } else {
            show(relativeTo: button)
        }
    }

    func show(relativeTo button: NSStatusBarButton, onFirstFrame: (@MainActor () -> Void)? = nil) {
        cascade.reset()
        guard let buttonWindow = button.window else { return }
        contentView?.layoutSubtreeIfNeeded()
        let height = fittedHeight()
        let anchor = buttonWindow.convertToScreen(button.convert(button.bounds, to: nil))
        let screen = buttonWindow.screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? .zero

        var x = anchor.midX - Self.width / 2
        x = min(max(x, screen.minX + 8), screen.maxX - Self.width - 8)
        let y = anchor.minY - height - 6
        var frame = NSRect(x: x, y: y, width: Self.width, height: height)
        // A signal-opened button can report a window that is not the menu-bar slot.
        // A click whose button has a real on-screen frame keeps this anchor.
        let anchoredMissesScreens = !NSScreen.screens.contains { $0.frame.intersects(frame) }
        if buttonWindow.screen == nil || button.bounds.height < 1 || anchoredMissesScreens,
           let visible = NSScreen.main?.visibleFrame {
            var fx = anchor.midX - Self.width / 2
            fx = min(max(fx, visible.minX + 8), visible.maxX - Self.width - 8)
            frame = NSRect(x: fx, y: visible.maxY - height - 6, width: Self.width, height: height)
        }
        setFrame(frame, display: true)
        scheduleRefit()
        animationBehavior = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
            ? .none
            : .utilityWindow

        frameCallback = onFirstFrame
        let opening = !isOpen
        isOpen = true
        if opening {
            onVisibility(true)
        }
        makeKeyAndOrderFront(nil)
        if let monitor = outsideClickMonitor {
            NSEvent.removeMonitor(monitor)
            outsideClickMonitor = nil
        }
        outsideClickMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self, weak button] event in
            // Global monitors report screen coordinates in locationInWindow.
            let clickPoint = event.locationInWindow
            Task { @MainActor in
                guard let self else { return }
                if let button, let buttonWindow = button.window {
                    let buttonFrame = buttonWindow.convertToScreen(button.convert(button.bounds, to: nil))
                    guard StatusPanelClickPolicy.shouldDismissPanel(
                        clickScreenPoint: clickPoint,
                        statusButtonScreenFrame: buttonFrame,
                        cascadeFrames: self.cascade.openFrames
                    ) else { return }
                }
                self.close()
            }
        }
        if frameCallback != nil {
            armFrameLink()
        }
    }

    override func close() {
        guard isOpen else { return }
        isOpen = false
        closedAt = CACurrentMediaTime()
        frameLink?.invalidate()
        frameLink = nil
        frameCallback = nil
        if let monitor = outsideClickMonitor {
            NSEvent.removeMonitor(monitor)
            outsideClickMonitor = nil
        }
        cascade.reset()
        onVisibility(false)
        orderOut(nil)
        super.close()
    }

    override func resignKey() {
        super.resignKey()
        if NSWorkspace.shared.isVoiceOverEnabled {
            DispatchQueue.main.async { [weak self] in
                guard let self, self.isOpen else { return }
                if self.cascade.keyIsInside { return }
                self.close()
            }
        } else {
            close()
        }
    }

    private func fittedHeight() -> CGFloat {
        let fitting = contentView?.fittingSize.height ?? 420
        return min(max(fitting, 1), Self.maxHeight)
    }

    private func scheduleRefit() {
        guard !refitPending else { return }
        refitPending = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.refitPending = false
            self.refitKeepingTop()
        }
    }

    private func refitKeepingTop() {
        guard isVisible else { return }
        let height = fittedHeight()
        guard abs(height - frame.height) > 0.5 else { return }
        var next = frame
        next.origin.y = frame.maxY - height
        next.size.height = height
        setFrame(next, display: true)
    }

    private func armFrameLink() {
        guard let callback = frameCallback else { return }
        guard let contentView else {
            frameCallback = nil
            callback()
            return
        }
        frameLink?.invalidate()
        let link = contentView.displayLink(target: self, selector: #selector(firstPresentedFrame(_:)))
        link.add(to: .main, forMode: .common)
        frameLink = link
    }

    @objc nonisolated func firstPresentedFrame(_ link: CADisplayLink) {
        link.invalidate()
        MainActor.assumeIsolated {
            frameLink = nil
            let callback = frameCallback
            frameCallback = nil
            callback?()
        }
    }

    override func cancelOperation(_ sender: Any?) {
        cascade.key(.escape)
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, routeKey(event) {
            return
        }
        super.sendEvent(event)
        guard event.type == .leftMouseUp else { return }
        activateRow(at: event.locationInWindow)
    }

    /// Hosting view is first responder and would eat arrows, Return, and Esc.
    private func routeKey(_ event: NSEvent) -> Bool {
        // ⌘-chords and VoiceOver (Control-Option / Caps Lock) must reach the system.
        if event.modifierFlags.contains(.command)
            || event.modifierFlags.contains(.control)
            || event.modifierFlags.contains(.option) {
            return false
        }
        guard let key = Self.cascadeKey(event.keyCode) else { return false }
        if firstResponder is NSText, key != .escape {
            return false
        }
        cascade.key(key)
        return true
    }

    private static func cascadeKey(_ keyCode: UInt16) -> CascadeKey? {
        switch keyCode {
        case 126: return .up
        case 125: return .down
        case 123: return .left
        case 124: return .right
        case 36, 76: return .return
        case 53: return .escape
        default: return nil
        }
    }

    /// SwiftUI Button does not see first mouse-up in this non-activating panel.
    private func activateRow(at windowPoint: CGPoint) {
        let pad: CGFloat = 5
        let row: CGFloat = 26
        let x = windowPoint.x
        let y = windowPoint.y
        guard x >= pad, x <= frame.width - pad else { return }
        let quit = pad..<(pad + row)
        let about = quit.upperBound..<(quit.upperBound + row)
        let settings = about.upperBound..<(about.upperBound + row)
        let divider: CGFloat = 1
        let coreStart = settings.upperBound + divider
        let core = coreStart..<(coreStart + row)
        let id: String
        switch y {
        case quit: id = "quit"
        case about: id = "about"
        case settings: id = "settings"
        case core: id = "core"
        default: return
        }
        cascade.click(id)
    }
}

private final class MenuHostingView<Content: View>: NSHostingView<Content> {
    var onFittingSizeChange: (() -> Void)?

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        super.hitTest(point) ?? self
    }

    override func layout() {
        super.layout()
        onFittingSizeChange?()
    }

    override func invalidateIntrinsicContentSize() {
        super.invalidateIntrinsicContentSize()
        onFittingSizeChange?()
    }
}
