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

/// Maps a point on the brief's 100-unit icon square (y down) onto the 1024 px canvas
/// (y up): one unit is 8.24 px of the 824 px body, whose origin is (100, 100).
func canvas(_ u: CGFloat, _ v: CGFloat) -> CGPoint {
    CGPoint(x: 100 + 8.24 * u, y: 924 - 8.24 * v)
}

/// A plane of the folded N. `pts` are on the brief's 18 pt grid (y down); the N is
/// translated by (19, 19) and scaled by 3.45 inside the 100-unit square.
func plane(_ pts: [(CGFloat, CGFloat)]) -> CGPath {
    let p = CGMutablePath()
    for (i, pt) in pts.enumerated() {
        let c = canvas(19 + 3.45 * pt.0, 19 + 3.45 * pt.1)
        if i == 0 { p.move(to: c) } else { p.addLine(to: c) }
    }
    p.closeSubpath()
    return p
}

let leftPlane = plane([(2.6, 6.5), (6.1, 11.6), (6.1, 15.4), (2.6, 15.4)])
let diagonalPlane = plane([(2.6, 2.6), (6.7, 2.6), (15.4, 15.4), (11.3, 15.4)])
let rightPlane = plane([(11.9, 2.6), (15.4, 2.6), (15.4, 11.5), (11.9, 6.4)])

func render(pixels: Int) -> Data {
    let cs = CGColorSpace(name: CGColorSpace.sRGB)!
    let ctx = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: 0,
                        space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let s = CGFloat(pixels) / 1024
    ctx.scaleBy(x: s, y: s)
    ctx.interpolationQuality = .high

    // Apple grid: 824 px body centred on the 1024 canvas.
    let body = CGRect(x: 100, y: 100, width: 824, height: 824)
    let cornerRadius: CGFloat = 23 * 8.24
    let bodyPath = CGPath(roundedRect: body, cornerWidth: cornerRadius, cornerHeight: cornerRadius, transform: nil)

    // Drop shadow.
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: color(0, 0, 0, 0.35))
    ctx.setFillColor(color(0, 0, 0))
    ctx.addPath(bodyPath)
    ctx.fillPath()
    ctx.restoreGState()

    // Body gradient (release: warm paper white, #fbfaf7 to #e3e0d8; dev: deep slate blue).
    ctx.saveGState()
    ctx.addPath(bodyPath)
    ctx.clip()
    let top = dev ? color(58, 74, 108) : color(251, 250, 247)
    let bottom = dev ? color(18, 24, 40) : color(227, 224, 216)
    let grad = CGGradient(colorsSpace: cs, colors: [top, bottom] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(grad, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
    // Soft inner top highlight (dev only: white on the light release body would vanish).
    if dev {
        let hl = CGGradient(colorsSpace: cs, colors: [color(255, 255, 255, 0.14), color(255, 255, 255, 0)] as CFArray,
                            locations: [0, 1])!
        ctx.drawLinearGradient(hl, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 620), options: [])
    }
    ctx.restoreGState()

    // Hairline edge.
    ctx.saveGState()
    ctx.setStrokeColor(dev ? color(255, 255, 255, 0.10) : color(26, 29, 36, 0.10))
    ctx.setLineWidth(3)
    ctx.addPath(CGPath(roundedRect: body.insetBy(dx: 1.5, dy: 1.5), cornerWidth: cornerRadius - 1.5, cornerHeight: cornerRadius - 1.5, transform: nil))
    ctx.strokePath()
    ctx.restoreGState()

    // The folded N. Release: Left and Right #8b8f99, Diagonal #1a1d24, with a soft
    // dark lift under the planes. Dev: light tones, Left and Right at half strength.
    let sides = dev ? color(246, 247, 250, 0.5) : color(139, 143, 153)
    let diagonal = dev ? color(246, 247, 250) : color(26, 29, 36)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -6), blur: 18,
                  color: dev ? color(255, 255, 255, 0.25) : color(26, 29, 36, 0.22))
    ctx.setFillColor(sides)
    ctx.addPath(leftPlane)
    ctx.addPath(rightPlane)
    ctx.fillPath()
    ctx.setFillColor(diagonal)
    ctx.addPath(diagonalPlane)
    ctx.fillPath()
    ctx.restoreGState()

    let img = ctx.makeImage()!
    return NSBitmapImageRep(cgImage: img).representation(using: .png, properties: [:])!
}

for base in [16, 32, 128, 256, 512] {
    try render(pixels: base).write(to: outDir.appendingPathComponent("icon_\(base)x\(base).png"))
    try render(pixels: base * 2).write(to: outDir.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
