// Draws Osom's app icon: the topics rail as a paper card on the accent
// squircle, with the agent's sparkle at its corner. Writes the iconset's
// PNGs into the directory it is given; `scripts/osom-icon.sh` packs them.
//
//   swift scripts/osom-icon.swift OUT.iconset
import AppKit

let canvas: CGFloat = 1024

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(
        srgbRed: CGFloat((hex >> 16) & 0xff) / 255,
        green: CGFloat((hex >> 8) & 0xff) / 255,
        blue: CGFloat(hex & 0xff) / 255,
        alpha: alpha)
}

/// macOS's icon shape: a superellipse filling the 824-point grid square.
func squircle() -> CGPath {
    let path = CGMutablePath()
    let (c, r, n): (CGFloat, CGFloat, CGFloat) = (512, 412, 5)
    for i in 0...720 {
        let t = CGFloat(i) / 720 * 2 * .pi
        let (cs, sn) = (cos(t), sin(t))
        let x = c + r * (cs < 0 ? -1 : 1) * pow(abs(cs), 2 / n)
        let y = c + r * (sn < 0 ? -1 : 1) * pow(abs(sn), 2 / n)
        i == 0 ? path.move(to: CGPoint(x: x, y: y)) : path.addLine(to: CGPoint(x: x, y: y))
    }
    path.closeSubpath()
    return path
}

/// A four-pointed sparkle, the mark the app gives the agent.
func sparkle(_ cx: CGFloat, _ cy: CGFloat, _ r: CGFloat) -> CGPath {
    let p = CGMutablePath()
    let k = r * 0.17
    p.move(to: CGPoint(x: cx, y: cy - r))
    p.addQuadCurve(to: CGPoint(x: cx + r, y: cy), control: CGPoint(x: cx + k, y: cy - k))
    p.addQuadCurve(to: CGPoint(x: cx, y: cy + r), control: CGPoint(x: cx + k, y: cy + k))
    p.addQuadCurve(to: CGPoint(x: cx - r, y: cy), control: CGPoint(x: cx - k, y: cy + k))
    p.addQuadCurve(to: CGPoint(x: cx, y: cy - r), control: CGPoint(x: cx - k, y: cy - k))
    return p
}

func rounded(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat, _ r: CGFloat) -> CGPath {
    CGPath(roundedRect: CGRect(x: x, y: y, width: w, height: h), cornerWidth: r, cornerHeight: r, transform: nil)
}

func draw(_ ctx: CGContext, px: Int) {
    let s = CGFloat(px) / canvas
    // Draw in a 1024-point space with y pointing down. Shadow offsets and
    // blurs ignore the transform, so they are given in pixels.
    ctx.translateBy(x: 0, y: CGFloat(px))
    ctx.scaleBy(x: s, y: -s)
    func shadow(_ dy: CGFloat, _ blur: CGFloat, _ c: CGColor) {
        ctx.setShadow(offset: CGSize(width: 0, height: -dy * s), blur: blur * s, color: c)
    }
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let shape = squircle()

    // The squircle, lit from the top left.
    ctx.saveGState()
    shadow(10, 22, color(0x000000, 0.28))
    ctx.addPath(shape)
    ctx.setFillColor(color(0x4b4ac2))
    ctx.fillPath()
    ctx.restoreGState()

    ctx.saveGState()
    ctx.addPath(shape)
    ctx.clip()
    let bg = CGGradient(colorsSpace: space, colors: [color(0x8080ec), color(0x5b5bd6), color(0x37369a)] as CFArray, locations: [0, 0.45, 1])!
    ctx.drawLinearGradient(bg, start: CGPoint(x: 220, y: 100), end: CGPoint(x: 804, y: 924), options: [])
    let sheen = CGGradient(colorsSpace: space, colors: [color(0xffffff, 0.16), color(0xffffff, 0)] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(sheen, start: CGPoint(x: 512, y: 100), end: CGPoint(x: 512, y: 520), options: [])

    // A second card behind, the rest of the topics.
    ctx.saveGState()
    ctx.translateBy(x: 482, y: 520)
    ctx.rotate(by: -8 * .pi / 180)
    ctx.addPath(rounded(-220, -236, 440, 472, 58))
    ctx.setFillColor(color(0xffffff, 0.2))
    ctx.fillPath()
    ctx.restoreGState()

    // The card: three topics, each a tile and two lines; the middle one open.
    let (x0, y0, w, h): (CGFloat, CGFloat, CGFloat, CGFloat) = (262, 300, 440, 440)
    ctx.saveGState()
    shadow(20, 44, color(0x14134a, 0.42))
    ctx.addPath(rounded(x0, y0, w, h, 58))
    ctx.setFillColor(color(0xfcfbf8))
    ctx.fillPath()
    ctx.restoreGState()

    let rows: [(tile: UInt32, title: CGFloat, hint: CGFloat)] = [
        (0xcf8558, 210, 150), (0x5b5bd6, 180, 196), (0x3f9a6e, 222, 128),
    ]
    for (i, row) in rows.enumerated() {
        let y = y0 + 52 + CGFloat(i) * 122
        if i == 1 {
            ctx.addPath(rounded(x0 + 22, y - 18, w - 44, 120, 30))
            ctx.setFillColor(color(0xe9e8fb))
            ctx.fillPath()
        }
        ctx.addPath(rounded(x0 + 50, y, 84, 84, 24))
        ctx.setFillColor(color(row.tile))
        ctx.fillPath()
        ctx.addPath(rounded(x0 + 160, y + 12, row.title, 28, 14))
        ctx.setFillColor(color(0x34322e))
        ctx.fillPath()
        ctx.addPath(rounded(x0 + 160, y + 54, row.hint, 20, 10))
        ctx.setFillColor(color(0xc8c4bc))
        ctx.fillPath()
    }

    // The agent's sparkle, glowing over the card's corner.
    ctx.saveGState()
    shadow(0, 34, color(0xffcf5a, 0.75))
    ctx.addPath(sparkle(716, 306, 132))
    ctx.setFillColor(color(0xffd66b))
    ctx.fillPath()
    ctx.restoreGState()
    ctx.saveGState()
    ctx.addPath(sparkle(716, 306, 132))
    ctx.clip()
    let gold = CGGradient(colorsSpace: space, colors: [color(0xfffaea), color(0xffd46a), color(0xf4ad2f)] as CFArray, locations: [0, 0.5, 1])!
    ctx.drawLinearGradient(gold, start: CGPoint(x: 640, y: 180), end: CGPoint(x: 800, y: 440), options: [])
    ctx.restoreGState()
    ctx.addPath(sparkle(812, 446, 40))
    ctx.setFillColor(color(0xfff1c4))
    ctx.fillPath()

    ctx.restoreGState()
}

func png(px: Int) -> Data {
    let ctx = CGContext(
        data: nil, width: px, height: px, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.interpolationQuality = .high
    draw(ctx, px: px)
    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    return rep.representation(using: .png, properties: [:])!
}

let out = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
for size in [16, 32, 128, 256, 512] {
    try png(px: size).write(to: out.appendingPathComponent("icon_\(size)x\(size).png"))
    try png(px: size * 2).write(to: out.appendingPathComponent("icon_\(size)x\(size)@2x.png"))
}
