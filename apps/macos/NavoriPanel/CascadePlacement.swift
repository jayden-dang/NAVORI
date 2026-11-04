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

    public static func submenuFrame(
        levelZero: CGRect,
        ancestorFrames: [CGRect],
        row: CGRect,
        size: CGSize,
        visible: CGRect
    ) -> CGRect {
        let parent = ancestorFrames.last ?? levelZero
        // Once the cascade has turned left it stays left, or this panel would cover its grandparent.
        if !ancestorFrames.isEmpty {
            let grandparent = ancestorFrames.dropLast().last ?? levelZero
            let left = parent.minX - gap - size.width
            if parent.minX < grandparent.minX, left >= visible.minX {
                let placed = frame(parent: parent, row: row, size: size, visible: visible)
                return CGRect(x: left, y: placed.minY, width: placed.width, height: placed.height)
            }
        }
        return frame(
            parent: parent,
            row: row,
            size: size,
            visible: visible
        )
    }

    public static func levelZeroHeight(fitting: CGFloat, visibleHeight: CGFloat) -> CGFloat {
        min(max(fitting, 1), max(visibleHeight, 1))
    }
}
