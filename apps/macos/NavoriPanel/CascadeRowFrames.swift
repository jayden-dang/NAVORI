import CoreGraphics

/// Where each row sits inside its panel. Screen positions are worked out when they are needed, because
/// a panel can move, even to another display, without any row changing inside it.
public struct CascadeRowFrames: Sendable {
    private var inPanel: [String: CGRect] = [:]

    public init() {}

    /// `frame` is in the panel's top-left coordinates, as SwiftUI reports it.
    public mutating func record(_ row: String, _ frame: CGRect) {
        inPanel[row] = frame
    }

    public func screenFrame(of row: String, in panelFrame: CGRect) -> CGRect? {
        guard let frame = inPanel[row] else { return nil }
        return CGRect(
            x: panelFrame.minX + frame.minX,
            y: panelFrame.maxY - frame.minY - frame.height,
            width: frame.width,
            height: frame.height
        )
    }
}
