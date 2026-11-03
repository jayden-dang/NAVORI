import AppKit
import NavoriPanel

/// Draws the status item's folded-N glyphs as 18 pt template images. The system tints a template, so no
/// color is chosen here; partial alpha carries the fold tone. Coordinates are the locked ones in
/// `docs/specs/2026-11-02-status-icon-states/ui-brief.md`, with y going down.
@MainActor
enum StatusIconImage {
    private struct Key: Hashable {
        let glyph: StatusIconGlyph
        let attention: Bool
    }

    private static var cache: [Key: NSImage] = [:]

    /// A copy of the cached template image for the state's glyph and attention, carrying its description.
    static func image(_ state: StatusIconState) -> NSImage {
        let key = Key(glyph: state.glyph, attention: state.attention)
        let base: NSImage
        if let cached = cache[key] {
            base = cached
        } else {
            base = draw(glyph: state.glyph, attention: state.attention)
            cache[key] = base
        }
        let copy = (base.copy() as? NSImage) ?? base
        copy.isTemplate = true
        copy.accessibilityDescription = state.description
        return copy
    }

    private static func draw(glyph: StatusIconGlyph, attention: Bool) -> NSImage {
        let image = NSImage(size: NSSize(width: 18, height: 18), flipped: true) { _ in
            guard let context = NSGraphicsContext.current?.cgContext else { return false }
            drawGlyph(glyph, in: context)
            if attention, hasMark(glyph) { drawMark(in: context) }
            return true
        }
        image.isTemplate = true
        return image
    }

    /// Core-down and Starting have no attention mark: no icon state exists then.
    private static func hasMark(_ glyph: StatusIconGlyph) -> Bool {
        switch glyph {
        case .normal, .lease, .aiRunning, .problem: true
        case .coreDown, .starting: false
        }
    }

    // MARK: Planes

    private static let left = [pt(2.6, 6.5), pt(6.1, 11.6), pt(6.1, 15.4), pt(2.6, 15.4)]
    private static let diagonal = [pt(2.6, 2.6), pt(6.7, 2.6), pt(15.4, 15.4), pt(11.3, 15.4)]
    private static let right = [pt(11.9, 2.6), pt(15.4, 2.6), pt(15.4, 11.5), pt(11.9, 6.4)]

    private static func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: x, y: y) }

    private static func drawGlyph(_ glyph: StatusIconGlyph, in c: CGContext) {
        switch glyph {
        case .normal:
            fill(left, alpha: 0.5, in: c)
            fill(diagonal, alpha: 1, in: c)
            fill(right, alpha: 0.5, in: c)
        case .lease:
            c.setFillColor(NSColor.black.cgColor)
            c.addPath(CGPath(
                roundedRect: CGRect(x: 1.3, y: 1.3, width: 15.4, height: 15.4),
                cornerWidth: 4.4, cornerHeight: 4.4, transform: nil
            ))
            c.fillPath()
            c.setBlendMode(.clear)
            for plane in [left, diagonal, right] {
                addPolygon(scaled(plane, by: 0.68), in: c)
                c.fillPath()
            }
            c.setBlendMode(.normal)
        case .aiRunning:
            fill(left, alpha: 1, in: c)
            fill(right, alpha: 1, in: c)
            for (from, to) in [(0.0, 0.26), (0.37, 0.63), (0.74, 1.0)] {
                fill(diagonalPiece(from: from, to: to), alpha: 1, in: c)
            }
        case .problem:
            fill([pt(9, 2.2), pt(16.4, 15.4), pt(1.6, 15.4)], alpha: 1, in: c)
            c.setBlendMode(.clear)
            c.setLineCap(.round)
            c.setLineWidth(1.8)
            c.move(to: pt(9, 6.8))
            c.addLine(to: pt(9, 10.8))
            c.move(to: pt(9, 13))
            c.addLine(to: pt(9, 13.4))
            c.strokePath()
            c.setBlendMode(.normal)
        case .coreDown:
            c.setLineWidth(3.1)
            c.setStrokeColor(NSColor.black.withAlphaComponent(0.5).cgColor)
            c.move(to: pt(14.8, 3.2))
            c.addLine(to: pt(10.4, 7.6))
            c.move(to: pt(7.6, 10.4))
            c.addLine(to: pt(3.2, 14.8))
            c.strokePath()
            c.setStrokeColor(NSColor.black.cgColor)
            c.move(to: pt(3.2, 3.2))
            c.addLine(to: pt(14.8, 14.8))
            c.strokePath()
        case .starting:
            c.setStrokeColor(NSColor.black.cgColor)
            c.setLineWidth(0.9)
            c.setLineJoin(.round)
            for plane in [left, diagonal, right] {
                addPolygon(plane, in: c)
                c.strokePath()
            }
        }
    }

    /// The attention mark: clear a ring, then fill the dot, so it reads against any glyph.
    private static func drawMark(in c: CGContext) {
        c.setBlendMode(.clear)
        c.fillEllipse(in: CGRect(x: 14.5 - 3.7, y: 3.5 - 3.7, width: 7.4, height: 7.4))
        c.setBlendMode(.normal)
        c.setFillColor(NSColor.black.cgColor)
        c.fillEllipse(in: CGRect(x: 14.5 - 2.5, y: 3.5 - 2.5, width: 5, height: 5))
    }

    // MARK: Helpers

    /// A slice of the diagonal between fractions `from` and `to` of its length.
    private static func diagonalPiece(from: CGFloat, to: CGFloat) -> [CGPoint] {
        func edges(_ t: CGFloat) -> (left: CGFloat, right: CGFloat, y: CGFloat) {
            (2.6 + 8.7 * t, 6.7 + 8.7 * t, 2.6 + 12.8 * t)
        }
        let a = edges(from), b = edges(to)
        return [pt(a.left, a.y), pt(a.right, a.y), pt(b.right, b.y), pt(b.left, b.y)]
    }

    private static func scaled(_ points: [CGPoint], by scale: CGFloat) -> [CGPoint] {
        points.map { pt(9 + ($0.x - 9) * scale, 9 + ($0.y - 9) * scale) }
    }

    private static func addPolygon(_ points: [CGPoint], in c: CGContext) {
        c.beginPath()
        c.addLines(between: points)
        c.closePath()
    }

    private static func fill(_ points: [CGPoint], alpha: CGFloat, in c: CGContext) {
        c.setFillColor(NSColor.black.withAlphaComponent(alpha).cgColor)
        addPolygon(points, in: c)
        c.fillPath()
    }
}
