// Renders the Navori app icon iconset. Usage: swift make-icon.swift <release|dev> <out.iconset>
// Driven by `cargo xtask icon`; the resulting .icns files are checked in.
import AppKit

let args = CommandLine.arguments
guard args.count == 3, ["release", "dev"].contains(args[1]) else {
    FileHandle.standardError.write(Data("usage: make-icon.swift <release|dev> <out.iconset>\n".utf8))
    exit(2)
}
let dev = args[1] == "dev"
let outDir = URL(fileURLWithPath: args[2])
try FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

func color(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> CGColor {
    CGColor(colorSpace: CGColorSpace(name: CGColorSpace.sRGB)!, components: [r / 255, g / 255, b / 255, a])!
}

/// Four-point sparkle with concave sides, centred on `c`.
func sparkle(center c: CGPoint, radius r: CGFloat, pinch: CGFloat) -> CGPath {
    let p = CGMutablePath()
    let tips = [CGPoint(x: c.x, y: c.y + r), CGPoint(x: c.x + r, y: c.y),
                CGPoint(x: c.x, y: c.y - r), CGPoint(x: c.x - r, y: c.y)]
    let ctrl = [CGPoint(x: c.x + pinch, y: c.y + pinch), CGPoint(x: c.x + pinch, y: c.y - pinch),
                CGPoint(x: c.x - pinch, y: c.y - pinch), CGPoint(x: c.x - pinch, y: c.y + pinch)]
    p.move(to: tips[0])
    for i in 0..<4 {
        p.addQuadCurve(to: tips[(i + 1) % 4], control: ctrl[i])
    }
    p.closeSubpath()
    return p
}

func render(pixels: Int) -> Data {
    let cs = CGColorSpace(name: CGColorSpace.sRGB)!
    let ctx = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: 0,
                        space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let s = CGFloat(pixels) / 1024
    ctx.scaleBy(x: s, y: s)
    ctx.interpolationQuality = .high

    // Apple grid: 824 px body centred on the 1024 canvas.
    let body = CGRect(x: 100, y: 100, width: 824, height: 824)
    let bodyPath = CGPath(roundedRect: body, cornerWidth: 186, cornerHeight: 186, transform: nil)

    // Drop shadow.
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: color(0, 0, 0, 0.35))
    ctx.setFillColor(color(0, 0, 0))
    ctx.addPath(bodyPath)
    ctx.fillPath()
    ctx.restoreGState()

    // Body gradient (top lighter graphite to near-black; dev is a deep slate blue).
    ctx.saveGState()
    ctx.addPath(bodyPath)
    ctx.clip()
    let top = dev ? color(58, 74, 108) : color(70, 72, 78)
    let bottom = dev ? color(18, 24, 40) : color(18, 18, 21)
    let grad = CGGradient(colorsSpace: cs, colors: [top, bottom] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(grad, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
    // Soft inner top highlight.
    let hl = CGGradient(colorsSpace: cs, colors: [color(255, 255, 255, 0.14), color(255, 255, 255, 0)] as CFArray,
                        locations: [0, 1])!
    ctx.drawLinearGradient(hl, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 620), options: [])
    ctx.restoreGState()

    // Hairline edge.
    ctx.saveGState()
    ctx.setStrokeColor(color(255, 255, 255, 0.10))
    ctx.setLineWidth(3)
    ctx.addPath(CGPath(roundedRect: body.insetBy(dx: 1.5, dy: 1.5), cornerWidth: 184.5, cornerHeight: 184.5, transform: nil))
    ctx.strokePath()
    ctx.restoreGState()

    // Sparkle glyph: large main star plus a small companion, with a soft glow.
    let main = sparkle(center: CGPoint(x: 488, y: 520), radius: 270, pinch: 34)
    let small = sparkle(center: CGPoint(x: 700, y: 726), radius: 92, pinch: 12)
    ctx.saveGState()
    ctx.setShadow(offset: .zero, blur: 40, color: color(255, 255, 255, 0.35))
    ctx.setFillColor(color(246, 247, 250))
    ctx.addPath(main)
    ctx.addPath(small)
    ctx.fillPath()
    ctx.restoreGState()

    let img = ctx.makeImage()!
    return NSBitmapImageRep(cgImage: img).representation(using: .png, properties: [:])!
}

for base in [16, 32, 128, 256, 512] {
    try render(pixels: base).write(to: outDir.appendingPathComponent("icon_\(base)x\(base).png"))
    try render(pixels: base * 2).write(to: outDir.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
