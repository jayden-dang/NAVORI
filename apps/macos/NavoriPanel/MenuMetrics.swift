import CoreGraphics

/// Geometry the level-0 menu, its hit map and the cascade tests share.
public enum MenuMetrics {
    public static let rowHeight: CGFloat = 26
    public static let pad: CGFloat = 5
    public static let divider: CGFloat = 1
    public static let level0Width: CGFloat = 360
}

/// First-mouse hits on the footer (and Core when shown). Window y = 0 is the bottom edge.
public enum MenuHitMap {
    public static func rowId(at point: CGPoint, width: CGFloat, tree: [CascadeNode]) -> String? {
        guard point.x >= MenuMetrics.pad, point.x <= width - MenuMetrics.pad else { return nil }
        let row = MenuMetrics.rowHeight
        var y = MenuMetrics.pad
        for id in tree.map(\.id).reversed() {
            if id == "running" { break }
            if id == "core" { y += MenuMetrics.divider }
            let band = y ..< (y + row)
            if band.contains(point.y) { return id }
            y += row
        }
        return nil
    }
}

extension CascadeKey {
    /// Hardware key codes for arrows, Return/keypad Enter, and Esc.
    public static func fromKeyCode(_ keyCode: UInt16) -> CascadeKey? {
        switch keyCode {
        case 126: .up
        case 125: .down
        case 123: .left
        case 124: .right
        case 36, 76: .return
        case 53: .escape
        default: nil
        }
    }
}
