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
    private var measuring = false
    /// Exact-once flag. `super.close()` can re-enter through `resignKey`.
    private(set) var isOpen = false
    /// Extra click can resign key (async close) then fire `toggle` and reopen.
    private var closedAt: CFTimeInterval = 0
    private static let width: CGFloat = MenuMetrics.level0Width
    private let hosting: MenuHostingView<AnyView>
    private let scroll: NSScrollView

    init(content: CascadeContent, onVisibility: @escaping (Bool) -> Void) {
        self.content = content
        self.onVisibility = onVisibility
        let cascade = CascadeController(
            tree: content.tree,
            submenu: content.submenu,
            activate: content.activate
        )
        self.cascade = cascade
        let hosting = MenuHostingView(rootView: AnyView(content.root.environment(cascade)))
        hosting.sizingOptions = [.intrinsicContentSize]
        self.hosting = hosting
        let clip = LevelZeroClipView()
        clip.drawsBackground = false
        let scroll = NSScrollView()
        scroll.contentView = clip
        scroll.drawsBackground = false
        scroll.backgroundColor = .clear
        scroll.borderType = .noBorder
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = false
        scroll.autohidesScrollers = true
        scroll.automaticallyAdjustsContentInsets = false
        scroll.documentView = hosting
        self.scroll = scroll
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

        hosting.onFittingSizeChange = { [weak self] in
            guard let self, !self.measuring else { return }
            self.scheduleRefit()
        }
        contentView = GlassSurfaceView(content: scroll)
    }

    override var canBecomeKey: Bool { true }

    func setTint(_ tint: NSColor) {
        if let glass = contentView as? GlassSurfaceView {
            glass.tint = tint
        }
        cascade.setTint(tint)
    }

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
        cascade.closeSubmenus()
        guard let buttonWindow = button.window else { return }
        let anchor = buttonWindow.convertToScreen(button.convert(button.bounds, to: nil))
        let screen = buttonWindow.screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? .zero
        let height = fittedHeight(visibleHeight: screen.height)

        var x = anchor.midX - Self.width / 2
        x = min(max(x, screen.minX + 8), screen.maxX - Self.width - 8)
        let y = anchor.minY - height - 6
        var frame = clampVertically(
            NSRect(x: x, y: y, width: Self.width, height: height),
            to: screen
        )
        // A signal-opened button can report a window that is not the menu-bar slot.
        // A click whose button has a real on-screen frame keeps this anchor.
        let anchoredMissesScreens = !NSScreen.screens.contains { $0.frame.intersects(frame) }
        if buttonWindow.screen == nil || button.bounds.height < 1 || anchoredMissesScreens,
           let visible = NSScreen.main?.visibleFrame {
            let fitted = fittedHeight(visibleHeight: visible.height)
            var fx = anchor.midX - Self.width / 2
            fx = min(max(fx, visible.minX + 8), visible.maxX - Self.width - 8)
            frame = clampVertically(
                NSRect(x: fx, y: visible.maxY - fitted - 6, width: Self.width, height: fitted),
                to: visible
            )
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
        cascade.closeSubmenus()
        onVisibility(false)
        orderOut(nil)
        super.close()
    }

    override func resignKey() {
        super.resignKey()
        DispatchQueue.main.async { [weak self] in
            guard let self, self.isOpen else { return }
            if self.cascade.keyIsInside { return }
            self.close()
        }
    }

    private func fittedHeight(visibleHeight: CGFloat) -> CGFloat {
        layoutDocument()
        let limit = visibleHeight > 1 ? visibleHeight : contentFittingHeight()
        return CascadePlacement.levelZeroHeight(
            fitting: contentFittingHeight(),
            visibleHeight: limit
        )
    }

    private func contentFittingHeight() -> CGFloat {
        measuring = true
        defer { measuring = false }
        hosting.invalidateIntrinsicContentSize()
        hosting.layoutSubtreeIfNeeded()
        return max(hosting.fittingSize.height, 1)
    }

    private func layoutDocument() {
        let height = contentFittingHeight()
        hosting.frame = NSRect(x: 0, y: 0, width: Self.width, height: height)
    }

    private func clampVertically(_ frame: NSRect, to visible: CGRect) -> NSRect {
        guard visible.height > 1 else { return frame }
        var next = frame
        if next.minY < visible.minY {
            next.origin.y = visible.minY
        }
        if next.maxY > visible.maxY {
            next.size.height = max(visible.maxY - next.minY, 1)
        }
        return next
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
        let visible = screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? .zero
        let height = fittedHeight(visibleHeight: visible.height)
        guard abs(height - frame.height) > 0.5 else { return }
        var next = frame
        next.origin.y = frame.maxY - height
        next.size.height = height
        next = clampVertically(next, to: visible)
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
        var modifiers = DecideModifiers()
        if event.modifierFlags.contains(.command) { modifiers.insert(.command) }
        if let decide = DecideKey.from(keyCode: event.keyCode, modifiers: modifiers),
           let selection = cascade.selection,
           selection.hasPrefix("decide."),
           selection != "decide.header"
        {
            switch decide {
            case .primary:
                cascade.click(selection)
            case .secondary:
                answerDecide(selection, primary: false)
            }
            return true
        }
        // ⌘-chords and VoiceOver (Control-Option / Caps Lock) must reach the system.
        if event.modifierFlags.contains(.command)
            || event.modifierFlags.contains(.control)
            || event.modifierFlags.contains(.option) {
            return false
        }
        guard let key = CascadeKey.fromKeyCode(event.keyCode) else { return false }
        if firstResponder is NSText, key != .escape {
            return false
        }
        cascade.key(key)
        return true
    }

    /// SwiftUI Button does not see first mouse-up in this non-activating panel.
    private func activateRow(at windowPoint: CGPoint) {
        guard let hit = MenuHitMap.hit(
            at: windowPoint,
            width: frame.width,
            tree: content.tree(),
            decideButtons: cascade.decideButtonFrames(),
            caughtUp: content.caughtUp()
        ) else { return }
        switch hit {
        case let .row(id):
            cascade.click(id)
        case let .decidePrimary(row):
            answerDecide(row, primary: true)
        case let .decideSecondary(row):
            answerDecide(row, primary: false)
        }
    }

    private func answerDecide(_ id: String, primary: Bool) {
        cascade.answerDecide?(id, primary)
    }
}

private final class LevelZeroClipView: NSClipView {
    override var isFlipped: Bool { true }
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
