// Draws Osom's app icon: a New Zealand brushtail possum peeking over a
// card of topics, on a sunrise squircle, with the agent's sparkle for a
// star. Writes the iconset's PNGs into the directory it is
// given; scripts/bundle-osom.sh packs them with iconutil.
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

func ellipse(_ cx: CGFloat, _ cy: CGFloat, _ rx: CGFloat, _ ry: CGFloat) -> CGPath {
    CGPath(ellipseIn: CGRect(x: cx - rx, y: cy - ry, width: 2 * rx, height: 2 * ry), transform: nil)
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
    func fill(_ path: CGPath, _ c: CGColor) {
        ctx.addPath(path)
        ctx.setFillColor(c)
        ctx.fillPath()
    }
    func gradient(_ path: CGPath, _ colors: [CGColor], from: CGPoint, to: CGPoint) {
        ctx.saveGState()
        ctx.addPath(path)
        ctx.clip()
        let g = CGGradient(colorsSpace: space, colors: colors as CFArray, locations: nil)!
        ctx.drawLinearGradient(g, start: from, end: to, options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
        ctx.restoreGState()
    }
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    let shape = squircle()

    // The sky: a sunrise squircle, gold to coral.
    ctx.saveGState()
    shadow(10, 22, color(0x000000, 0.28))
    fill(shape, color(0xff7d52))
    ctx.restoreGState()
    ctx.saveGState()
    ctx.addPath(shape)
    ctx.clip()
    let sky = CGGradient(colorsSpace: space, colors: [color(0xffcf5c), color(0xff9a4d), color(0xff5f6d)] as CFArray, locations: [0, 0.5, 1])!
    ctx.drawLinearGradient(sky, start: CGPoint(x: 300, y: 100), end: CGPoint(x: 724, y: 924), options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])

    // The agent's sparkle, a star over the possum's shoulder.
    ctx.saveGState()
    shadow(0, 26, color(0xffffff, 0.7))
    fill(sparkle(770, 236, 82), color(0xfff6e0))
    ctx.restoreGState()
    gradient(sparkle(770, 236, 82), [color(0xffffff), color(0xfff3d6), color(0xffe2a8)], from: CGPoint(x: 720, y: 160), to: CGPoint(x: 820, y: 320))
    fill(sparkle(842, 344, 26), color(0xfffaf0))

    // A brushtail possum peeking over the card: tall ears, a pointed pale
    // snout, big night eyes, pink fingers on the card's edge, and the bushy
    // black-tipped tail sweeping up behind it.
    let fur = [color(0xb5ada6), color(0x908882), color(0x6f6862)]
    let (x0, y0, w, h): (CGFloat, CGFloat, CGFloat, CGFloat) = (242, 676, 540, 204)
    func tailPoint(_ t: CGFloat) -> CGPoint {
        let (p0, p1, p2) = (CGPoint(x: 330, y: 830), CGPoint(x: 150, y: 830), CGPoint(x: 214, y: 540))
        let u = 1 - t
        return CGPoint(x: u * u * p0.x + 2 * u * t * p1.x + t * t * p2.x, y: u * u * p0.y + 2 * u * t * p1.y + t * t * p2.y)
    }
    ctx.saveGState()
    shadow(10, 24, color(0x7a2a1a, 0.35))
    for i in 0...40 {
        let t = CGFloat(i) / 40
        let p = tailPoint(t)
        let r = 56 - 14 * t + 10 * sin(t * .pi)
        fill(ellipse(p.x, p.y, r, r), t > 0.86 ? color(0x24201f) : color(0x5a534e))
    }
    ctx.restoreGState()
    for i in 0...40 {
        let t = CGFloat(i) / 40
        let p = tailPoint(t)
        let r = 56 - 14 * t + 10 * sin(t * .pi)
        fill(ellipse(p.x + 6, p.y - 8, r * 0.68, r * 0.68), t > 0.86 ? color(0x35302d) : color(0x7a726c))
    }

    for side: CGFloat in [-1, 1] {
        ctx.saveGState()
        ctx.translateBy(x: 512 + side * 150, y: 262)
        ctx.rotate(by: side * 18 * .pi / 180)
        gradient(ellipse(0, 0, 66, 116), fur, from: CGPoint(x: 0, y: -116), to: CGPoint(x: 0, y: 116))
        fill(ellipse(0, 16, 38, 80), color(0xe6a1a0))
        ctx.restoreGState()
    }
    let head = CGMutablePath()
    head.addPath(ellipse(512, 440, 214, 178))
    head.addPath(ellipse(512, 560, 128, 104))
    ctx.saveGState()
    shadow(14, 30, color(0x7a2a1a, 0.35))
    fill(head, color(0x908882))
    ctx.restoreGState()
    gradient(head, fur, from: CGPoint(x: 512, y: 262), to: CGPoint(x: 512, y: 664))
    // The pale snout and cheeks.
    ctx.saveGState()
    ctx.addPath(head)
    ctx.clip()
    fill(ellipse(512, 590, 116, 104), color(0xeee7de))
    fill(ellipse(412, 548, 70, 46), color(0xe2dad1))
    fill(ellipse(612, 548, 70, 46), color(0xe2dad1))
    ctx.restoreGState()
    // Big night eyes in a soft dark mask.
    for side: CGFloat in [-1, 1] {
        let (x, y): (CGFloat, CGFloat) = (512 + side * 98, 446)
        fill(ellipse(x, y + 4, 68, 64), color(0x4a4440, 0.38))
        fill(ellipse(x, y, 52, 54), color(0x141216))
        fill(ellipse(x + 16, y - 19, 17, 17), color(0xffffff, 0.95))
        fill(ellipse(x - 17, y + 19, 7, 7), color(0xffffff, 0.55))
    }
    // A pink nose at the snout's tip, and whiskers.
    let nose = CGMutablePath()
    nose.move(to: CGPoint(x: 476, y: 600))
    nose.addQuadCurve(to: CGPoint(x: 548, y: 600), control: CGPoint(x: 512, y: 576))
    nose.addQuadCurve(to: CGPoint(x: 512, y: 644), control: CGPoint(x: 546, y: 632))
    nose.addQuadCurve(to: CGPoint(x: 476, y: 600), control: CGPoint(x: 478, y: 632))
    fill(nose, color(0xe0828f))
    fill(ellipse(502, 606, 11, 7), color(0xffffff, 0.55))
    ctx.setStrokeColor(color(0xf7f3ee, 0.75))
    ctx.setLineWidth(4)
    ctx.setLineCap(.round)
    for side: CGFloat in [-1, 1] {
        for (dy, reach) in [(CGFloat(-14), CGFloat(150)), (6, 160), (26, 140)] {
            ctx.move(to: CGPoint(x: 512 + side * 64, y: 622 + dy * 0.4))
            ctx.addQuadCurve(to: CGPoint(x: 512 + side * (64 + reach), y: 610 + dy * 1.6), control: CGPoint(x: 512 + side * (64 + reach * 0.5), y: 612 + dy))
        }
    }
    ctx.strokePath()

    // The card it holds: the topics, a tile and two lines each.
    ctx.saveGState()
    shadow(18, 40, color(0x7a2a1a, 0.45))
    fill(rounded(x0, y0, w, h, 50), color(0xfcfbf8))
    ctx.restoreGState()
    let rows: [(tile: UInt32, title: CGFloat, hint: CGFloat)] = [(0xcf8558, 250, 170), (0x3f9a6e, 210, 200)]
    for (i, row) in rows.enumerated() {
        let y = y0 + 52 + CGFloat(i) * 84
        fill(rounded(x0 + 54, y, 60, 60, 18), color(row.tile))
        fill(rounded(x0 + 144, y + 8, row.title, 22, 11), color(0x34322e))
        fill(rounded(x0 + 144, y + 40, row.hint, 16, 8), color(0xc8c4bc))
    }
    // Hands on the card's edge: grey palms, four pink fingers each.
    for side: CGFloat in [-1, 1] {
        let x = 512 + side * 128
        ctx.saveGState()
        shadow(6, 10, color(0x7a2a1a, 0.25))
        gradient(ellipse(x, y0 - 6, 54, 34), [fur[0], fur[1]], from: CGPoint(x: x, y: y0 - 40), to: CGPoint(x: x, y: y0 + 28))
        ctx.restoreGState()
        for f in 0..<4 {
            let fx = x - 36 + CGFloat(f) * 24
            fill(ellipse(fx, y0 + 18, 11, 17), color(0xe9a5a6))
        }
    }

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
