import CoreGraphics

public enum CascadePlacement {
    public static let gap: CGFloat = 4

    public static func frame(
        parent: CGRect,
        row: CGRect,
        size: CGSize,
        visible: CGRect
    ) -> CGRect {
        var x = parent.maxX + gap
        if x + size.width > visible.maxX {
            x = parent.minX - gap - size.width
        }
        var height = size.height
        var y = row.maxY - height
        if y < visible.minY {
            y = visible.minY
        }
        if y + height > visible.maxY {
            height = visible.maxY - y
        }
        return CGRect(x: x, y: y, width: size.width, height: height)
    }
}
