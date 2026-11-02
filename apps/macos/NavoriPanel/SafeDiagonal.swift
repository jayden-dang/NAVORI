import CoreGraphics

public enum SafeDiagonal {
    public static func contains(
        _ point: CGPoint,
        from anchor: CGPoint,
        toward submenu: CGRect
    ) -> Bool {
        let nearX = submenu.maxX <= anchor.x ? submenu.maxX : submenu.minX
        return inTriangle(
            point,
            anchor,
            CGPoint(x: nearX, y: submenu.maxY),
            CGPoint(x: nearX, y: submenu.minY)
        )
    }

    private static func inTriangle(
        _ p: CGPoint,
        _ a: CGPoint,
        _ b: CGPoint,
        _ c: CGPoint
    ) -> Bool {
        let area = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y)
        guard area != 0 else { return false }
        let u = ((b.y - c.y) * (p.x - c.x) + (c.x - b.x) * (p.y - c.y)) / area
        let v = ((c.y - a.y) * (p.x - c.x) + (a.x - c.x) * (p.y - c.y)) / area
        let w = 1 - u - v
        return u >= 0 && v >= 0 && w >= 0
    }
}
