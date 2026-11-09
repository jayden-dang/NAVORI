import CoreGraphics

/// Geometry the level-0 menu, its hit map and the cascade tests share.
public enum MenuMetrics {
    public static let rowHeight: CGFloat = 26
    public static let pad: CGFloat = 5
    public static let divider: CGFloat = 1
    public static let level0Width: CGFloat = 360
}

public struct DecideButtonFrames: Equatable, Sendable {
    public var primary: CGRect
    public var secondary: CGRect

    public init(primary: CGRect, secondary: CGRect) {
        self.primary = primary
        self.secondary = secondary
    }
}

public enum MenuHit: Equatable, Sendable {
    case row(String)
    case decidePrimary(row: String)
    case decideSecondary(row: String)
}

/// First-mouse hits on level-0 rows (footer, Core, Needs you, Decide) and Decide choice buttons.
/// Window y = 0 is the bottom edge. `running` occupies space but is not a first-mouse target.
public enum MenuHitMap {
    public static func hit(
        at point: CGPoint,
        width: CGFloat,
        tree: [CascadeNode],
        decideButtons: [String: DecideButtonFrames] = [:],
        caughtUp: Bool = false
    ) -> MenuHit? {
        guard point.x >= MenuMetrics.pad, point.x <= width - MenuMetrics.pad else { return nil }
        let row = MenuMetrics.rowHeight
        var y = MenuMetrics.pad
        for id in tree.map(\.id).reversed() {
            if id == "core" { y += MenuMetrics.divider }
            if id == "needs" { y += MenuMetrics.divider }
            let band = y ..< (y + row)
            defer {
                y += row
                if id == "needs", caughtUp { y += row }
            }
            guard band.contains(point.y) else { continue }
            if id == "running" { return nil }
            if let buttons = decideButtons[id] {
                if buttons.primary.contains(point) { return .decidePrimary(row: id) }
                if buttons.secondary.contains(point) { return .decideSecondary(row: id) }
            }
            return .row(id)
        }
        return nil
    }

    public static func rowId(at point: CGPoint, width: CGFloat, tree: [CascadeNode]) -> String? {
        switch hit(at: point, width: width, tree: tree) {
        case let .row(id), let .decidePrimary(id), let .decideSecondary(id):
            id
        case nil:
            nil
        }
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
