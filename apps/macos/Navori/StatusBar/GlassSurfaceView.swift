import AppKit

enum GlassTint {
    static let standard = NSColor(name: "GlassTint.standard", dynamicProvider: { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor.black.withAlphaComponent(0.69)
            : NSColor.white.withAlphaComponent(0.57)
    })
}

final class GlassSurfaceView: NSView {
    var tint: NSColor {
        didSet { glass.tintColor = tint }
    }

    private let glass = FirstMouseGlassView()
    private let content: NSView
    private var showingOpaque = false
    private var didApply = false
    private var observer: (any NSObjectProtocol)?

    init(content: NSView) {
        self.content = content
        self.tint = GlassTint.standard
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 12
        layer?.masksToBounds = true
        glass.style = .clear
        glass.cornerRadius = 12
        glass.tintColor = tint
        glass.translatesAutoresizingMaskIntoConstraints = false
        content.translatesAutoresizingMaskIntoConstraints = false
        observer = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification,
            object: NSWorkspace.shared,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.applySurface()
            }
        }
        applySurface()
    }

    isolated deinit {
        if let observer {
            NSWorkspace.shared.notificationCenter.removeObserver(observer)
        }
    }

    required init?(coder: NSCoder) { nil }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        super.hitTest(point) ?? self
    }

    override var fittingSize: NSSize { content.fittingSize }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        layer?.cornerRadius = 12
        layer?.masksToBounds = true
        layer?.backgroundColor = showingOpaque
            ? NSColor.windowBackgroundColor.cgColor
            : NSColor.black.withAlphaComponent(0.01).cgColor
    }

    private func applySurface() {
        let reduce = NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency
        if didApply, reduce == showingOpaque { return }
        didApply = true
        if reduce {
            showOpaque()
        } else {
            showGlass()
        }
    }

    private func showGlass() {
        showingOpaque = false
        needsDisplay = true
        if glass.contentView !== content {
            content.removeFromSuperview()
            glass.contentView = content
        }
        if glass.superview !== self {
            addSubview(glass)
            NSLayoutConstraint.activate([
                glass.leadingAnchor.constraint(equalTo: leadingAnchor),
                glass.trailingAnchor.constraint(equalTo: trailingAnchor),
                glass.topAnchor.constraint(equalTo: topAnchor),
                glass.bottomAnchor.constraint(equalTo: bottomAnchor),
            ])
        }
        pin(content, to: glass)
        glass.isHidden = false
    }

    private func showOpaque() {
        showingOpaque = true
        glass.contentView = nil
        glass.removeFromSuperview()
        if content.superview !== self {
            addSubview(content)
            pin(content, to: self)
        }
        needsDisplay = true
    }

    private func pin(_ child: NSView, to parent: NSView) {
        NSLayoutConstraint.activate([
            child.leadingAnchor.constraint(equalTo: parent.leadingAnchor),
            child.trailingAnchor.constraint(equalTo: parent.trailingAnchor),
            child.topAnchor.constraint(equalTo: parent.topAnchor),
            child.bottomAnchor.constraint(equalTo: parent.bottomAnchor),
        ])
    }

}

private final class FirstMouseGlassView: NSGlassEffectView {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        if let contentView {
            let local = convert(point, to: contentView)
            if let hit = contentView.hitTest(local) {
                return hit
            }
        }
        return super.hitTest(point) ?? self
    }
}
