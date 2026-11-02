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
    private static let width: CGFloat = 360
    private static let maxHeight: CGFloat = 560

    init(content: CascadeContent, onVisibility: @escaping (Bool) -> Void) {
        self.content = content
        self.onVisibility = onVisibility
        self.cascade = CascadeController(tree: content.tree, activate: content.activate)
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
        backgroundColor = .clear
        isOpaque = false
        hasShadow = true
        animationBehavior = .utilityWindow

        let hosting = MenuHostingView(rootView: self.content.root.environment(self.cascade))
        contentView = GlassSurfaceView(content: hosting)
    }

    override var canBecomeKey: Bool { true }

    func toggle(relativeTo button: NSStatusBarButton) {
        isVisible ? close() : show(relativeTo: button)
    }

    func show(relativeTo button: NSStatusBarButton, onFirstFrame: (@MainActor () -> Void)? = nil) {
        cascade.reset()
        guard let buttonWindow = button.window else { return }
        let fitting = contentView?.fittingSize.height ?? 420
        let height = min(max(fitting, 1), Self.maxHeight)
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

        frameCallback = onFirstFrame
        onVisibility(true)
        makeKeyAndOrderFront(nil)
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
                        cascadeFrames: []
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
        frameLink?.invalidate()
        frameLink = nil
        frameCallback = nil
        if let monitor = outsideClickMonitor {
            NSEvent.removeMonitor(monitor)
            outsideClickMonitor = nil
        }
        cascade.reset()
        onVisibility(false)
        super.close()
    }

    override func resignKey() {
        super.resignKey()
        close()
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
        close()
    }
}

private final class MenuHostingView<Content: View>: NSHostingView<Content> {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}
