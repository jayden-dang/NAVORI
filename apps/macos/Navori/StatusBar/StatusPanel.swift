import AppKit
import SwiftUI

/// Borderless, non-activating panel anchored under the menubar item — the Navori "popover".
/// Closes on Escape, on click outside, and when it loses key status.
@MainActor
final class StatusPanel: NSPanel {
    private let store: CoreStore
    private var outsideClickMonitor: Any?
    private static let width: CGFloat = 380
    private static let maxHeight: CGFloat = 560

    init(store: CoreStore, openSettings: @escaping () -> Void) {
        self.store = store
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: Self.width, height: 420),
            styleMask: [.nonactivatingPanel, .borderless, .fullSizeContentView],
            backing: .buffered,
            defer: true
        )
        isFloatingPanel = true
        level = .popUpMenu
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        hidesOnDeactivate = false
        isMovable = false
        backgroundColor = .clear
        isOpaque = false
        hasShadow = true
        animationBehavior = .utilityWindow

        let background = NSVisualEffectView()
        background.material = .popover
        background.state = .active
        background.blendingMode = .behindWindow
        background.wantsLayer = true
        background.layer?.cornerRadius = 12
        background.layer?.masksToBounds = true

        let hosting = NSHostingView(rootView: PanelView(store: store, openSettings: openSettings))
        hosting.translatesAutoresizingMaskIntoConstraints = false
        background.addSubview(hosting)
        NSLayoutConstraint.activate([
            hosting.leadingAnchor.constraint(equalTo: background.leadingAnchor),
            hosting.trailingAnchor.constraint(equalTo: background.trailingAnchor),
            hosting.topAnchor.constraint(equalTo: background.topAnchor),
            hosting.bottomAnchor.constraint(equalTo: background.bottomAnchor),
        ])
        contentView = background
    }

    override var canBecomeKey: Bool { true }

    func toggle(relativeTo button: NSStatusBarButton) {
        isVisible ? close() : show(relativeTo: button)
    }

    func show(relativeTo button: NSStatusBarButton) {
        guard let buttonWindow = button.window else { return }
        let fitting = contentView?.fittingSize.height ?? 420
        let height = min(max(fitting, 200), Self.maxHeight)
        let anchor = buttonWindow.convertToScreen(button.convert(button.bounds, to: nil))
        let screen = buttonWindow.screen?.visibleFrame ?? NSScreen.main?.visibleFrame ?? .zero

        var x = anchor.midX - Self.width / 2
        x = min(max(x, screen.minX + 8), screen.maxX - Self.width - 8)
        let y = anchor.minY - height - 6
        setFrame(NSRect(x: x, y: y, width: Self.width, height: height), display: true)

        store.setPanelVisible(true)
        makeKeyAndOrderFront(nil)
        outsideClickMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
            Task { @MainActor in self?.close() }
        }
    }

    override func close() {
        if let monitor = outsideClickMonitor {
            NSEvent.removeMonitor(monitor)
            outsideClickMonitor = nil
        }
        store.setPanelVisible(false)
        super.close()
    }

    override func resignKey() {
        super.resignKey()
        close()
    }

    override func cancelOperation(_ sender: Any?) {
        close()
    }
}
