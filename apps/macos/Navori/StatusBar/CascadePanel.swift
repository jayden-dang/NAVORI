import AppKit
import QuartzCore
import SwiftUI

@MainActor
final class CascadePanel: NSPanel {
    var onFittingSizeChange: (() -> Void)?

    private let hosting: CascadeHostingView
    private let scroll: NSScrollView
    private var frameLink: CADisplayLink?
    private var frameCallback: (@MainActor () -> Void)?
    private var measuring = false

    init() {
        hosting = CascadeHostingView(rootView: AnyView(EmptyView()))
        hosting.sizingOptions = [.intrinsicContentSize]
        let clip = FlippedClipView()
        clip.drawsBackground = false
        scroll = NSScrollView()
        scroll.contentView = clip
        scroll.drawsBackground = false
        scroll.backgroundColor = .clear
        scroll.borderType = .noBorder
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = false
        scroll.autohidesScrollers = true
        scroll.automaticallyAdjustsContentInsets = false
        scroll.documentView = hosting
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 360, height: 200),
            styleMask: [.nonactivatingPanel, .borderless, .fullSizeContentView],
            backing: .buffered,
            defer: true
        )
        isReleasedWhenClosed = false
        animationBehavior = .none
        isFloatingPanel = true
        level = .popUpMenu
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        hidesOnDeactivate = false
        isMovable = false
        acceptsMouseMovedEvents = true
        backgroundColor = NSColor.black.withAlphaComponent(0.01)
        isOpaque = false
        hasShadow = true
        contentView = GlassSurfaceView(content: scroll)
        hosting.onFittingSizeChange = { [weak self] in
            guard let self, !self.measuring else { return }
            self.onFittingSizeChange?()
        }
    }

    required init?(coder: NSCoder) { nil }

    override var canBecomeKey: Bool { false }

    func setRootView(_ view: AnyView) {
        hosting.rootView = view
        layoutDocument()
    }

    var idealSize: CGSize {
        measuring = true
        defer { measuring = false }
        layoutDocument()
        return CGSize(width: 360, height: max(hosting.fittingSize.height, 1))
    }

    func show(frame: CGRect, onFirstFrame: (@MainActor () -> Void)? = nil) {
        layoutDocument()
        setFrame(frame, display: true)
        layoutDocument()
        orderFrontRegardless()
        if let onFirstFrame {
            frameCallback = onFirstFrame
            armFrameLink()
        }
    }

    func hide() {
        frameLink?.invalidate()
        frameLink = nil
        frameCallback = nil
        orderOut(nil)
    }

    private func layoutDocument() {
        hosting.invalidateIntrinsicContentSize()
        hosting.layoutSubtreeIfNeeded()
        let height = max(hosting.fittingSize.height, 1)
        hosting.frame = NSRect(x: 0, y: 0, width: 360, height: height)
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
}

private final class FlippedClipView: NSClipView {
    override var isFlipped: Bool { true }
}

private final class CascadeHostingView: NSHostingView<AnyView> {
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
