// Draws Osom's app icon (Coastal Lagoon & Brass): a poised New Zealand
// brushtail possum framed by a sunlit cream disc, peeking over a honey-brass
// folio with a terracotta tab and the agent's four-point spark.
// Writes the iconset's PNGs into the directory it is given;
// scripts/bundle-osom.sh packs them with iconutil.
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

func rounded(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat, _ r: CGFloat) -> CGPath {
    CGPath(roundedRect: CGRect(x: x, y: y, width: w, height: h), cornerWidth: r, cornerHeight: r, transform: nil)
}

func ellipse(_ cx: CGFloat, _ cy: CGFloat, _ rx: CGFloat, _ ry: CGFloat) -> CGPath {
    CGPath(ellipseIn: CGRect(x: cx - rx, y: cy - ry, width: 2 * rx, height: 2 * ry), transform: nil)
}

func draw(_ ctx: CGContext, px: Int, fullBleed: Bool = false) {
    let s = CGFloat(px) / canvas
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    // Draw in a 1024-point space with y pointing down. Shadow offsets and
    // blurs ignore the transform, so they are given in pixels.
    ctx.translateBy(x: 0, y: CGFloat(px))
    ctx.scaleBy(x: s, y: -s)

    func fill(_ path: CGPath, _ c: CGColor) {
        ctx.addPath(path)
        ctx.setFillColor(c)
        ctx.fillPath()
    }
    func stroke(_ path: CGPath, _ c: CGColor, width: CGFloat, cap: CGLineCap = .round) {
        ctx.addPath(path)
        ctx.setStrokeColor(c)
        ctx.setLineWidth(width)
        ctx.setLineCap(cap)
        ctx.strokePath()
    }
    func gradient(_ path: CGPath, _ colors: [CGColor], from: CGPoint, to: CGPoint) {
        ctx.saveGState()
        ctx.addPath(path)
        ctx.clip()
        let g = CGGradient(colorsSpace: space, colors: colors as CFArray, locations: nil)!
        ctx.drawLinearGradient(g, start: from, end: to, options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
        ctx.restoreGState()
    }

    let shape = squircle()

    if !fullBleed {
        // macOS squircle drop shadow.
        ctx.saveGState()
        ctx.setShadow(offset: CGSize(width: 0, height: -10 * s), blur: 22 * s, color: color(0x000000, 0.28))
        fill(shape, color(0x125066))
        ctx.restoreGState()
    }

    ctx.saveGState()
    if !fullBleed {
        ctx.addPath(shape)
        ctx.clip()
        ctx.saveGState()
        ctx.translateBy(x: 100, y: 100)
        ctx.scaleBy(x: 824 / 120, y: 824 / 120)
    } else {
        ctx.saveGState()
        ctx.scaleBy(x: canvas / 120, y: canvas / 120)
    }

    // Backdrop: coastal lagoon gradient.
    let full = CGPath(rect: CGRect(x: 0, y: 0, width: 120, height: 120), transform: nil)
    gradient(full, [color(0x207f99), color(0x125066)], from: CGPoint(x: 0, y: 0), to: CGPoint(x: 0, y: 120))

    // Luminous warm cream disc behind the possum for high-contrast small-Dock legibility.
    gradient(ellipse(60, 51, 44, 44), [color(0xfffef9), color(0xf7efd8)], from: CGPoint(x: 60, y: 7), to: CGPoint(x: 60, y: 95))

    // Subtle terracotta folder tab peeking behind the right side of the folio.
    let tab = CGMutablePath()
    tab.move(to: CGPoint(x: 85, y: 78))
    tab.addLine(to: CGPoint(x: 87, y: 67))
    tab.addCurve(to: CGPoint(x: 93, y: 62.5), control1: CGPoint(x: 87.8, y: 64), control2: CGPoint(x: 90, y: 62.5))
    tab.addLine(to: CGPoint(x: 106, y: 62.5))
    tab.addCurve(to: CGPoint(x: 111, y: 67.5), control1: CGPoint(x: 109, y: 62.5), control2: CGPoint(x: 111, y: 64.5))
    tab.addLine(to: CGPoint(x: 111, y: 78))
    tab.closeSubpath()
    fill(tab, color(0xc9684b))

    // NZ brushtail possum: alert rounded ears, sculpted slate-charcoal fur,
    // cream face mask with dark nose-bridge blaze, calm eyes, understated nose.
    let fur = color(0x25292e)
    let furLight = color(0x4c555e, 0.35)
    let earInner = color(0xcf8f7a)
    let faceCream = color(0xfcfbf6)
    let eye = color(0x14171a)
    let noseColor = color(0x9e5f4d)

    // Left and right ears.
    for (cx, cy, ix, deg): (CGFloat, CGFloat, CGFloat, CGFloat) in [
        (34, 32, 35.1, -20),
        (86, 32, 84.9, 20),
    ] {
        ctx.saveGState()
        ctx.translateBy(x: cx, y: cy)
        ctx.rotate(by: deg * .pi / 180)
        ctx.translateBy(x: -cx, y: -cy)
        fill(ellipse(cx, cy, 9.8, 16.5), fur)
        fill(ellipse(ix, 33.5, 4.9, 10.2), earInner)
        ctx.restoreGState()
    }

    // Composed head silhouette.
    let head = CGMutablePath()
    head.move(to: CGPoint(x: 60, y: 25))
    head.addCurve(to: CGPoint(x: 87, y: 52), control1: CGPoint(x: 76, y: 25), control2: CGPoint(x: 87, y: 36))
    head.addCurve(to: CGPoint(x: 60, y: 79), control1: CGPoint(x: 87, y: 67), control2: CGPoint(x: 74.5, y: 79))
    head.addCurve(to: CGPoint(x: 33, y: 52), control1: CGPoint(x: 45.5, y: 79), control2: CGPoint(x: 33, y: 67))
    head.addCurve(to: CGPoint(x: 60, y: 25), control1: CGPoint(x: 33, y: 36), control2: CGPoint(x: 44, y: 25))
    head.closeSubpath()
    fill(head, fur)

    // Subtle crown highlight plane.
    let crown = CGMutablePath()
    crown.move(to: CGPoint(x: 46, y: 29))
    crown.addCurve(to: CGPoint(x: 74, y: 29), control1: CGPoint(x: 51, y: 26.8), control2: CGPoint(x: 69, y: 26.8))
    crown.addCurve(to: CGPoint(x: 60, y: 36), control1: CGPoint(x: 70, y: 34), control2: CGPoint(x: 65, y: 36))
    crown.addCurve(to: CGPoint(x: 46, y: 29), control1: CGPoint(x: 55, y: 36), control2: CGPoint(x: 50, y: 34))
    crown.closeSubpath()
    fill(crown, furLight)

    // Cheek mask with authentic dark brushtail nose-bridge blaze.
    let mask = CGMutablePath()
    mask.move(to: CGPoint(x: 35.5, y: 54))
    mask.addCurve(to: CGPoint(x: 53.5, y: 48.5), control1: CGPoint(x: 38.5, y: 45.5), control2: CGPoint(x: 46, y: 44.5))
    mask.addCurve(to: CGPoint(x: 58.6, y: 58.5), control1: CGPoint(x: 56.5, y: 50.2), control2: CGPoint(x: 58.2, y: 53.5))
    mask.addLine(to: CGPoint(x: 61.4, y: 58.5))
    mask.addCurve(to: CGPoint(x: 66.5, y: 48.5), control1: CGPoint(x: 61.8, y: 53.5), control2: CGPoint(x: 63.5, y: 50.2))
    mask.addCurve(to: CGPoint(x: 84.5, y: 54), control1: CGPoint(x: 74, y: 44.5), control2: CGPoint(x: 81.5, y: 45.5))
    mask.addCurve(to: CGPoint(x: 60, y: 78), control1: CGPoint(x: 83, y: 67.5), control2: CGPoint(x: 72.5, y: 78))
    mask.addCurve(to: CGPoint(x: 35.5, y: 54), control1: CGPoint(x: 47.5, y: 78), control2: CGPoint(x: 37, y: 67.5))
    mask.closeSubpath()
    fill(mask, faceCream)

    // Calm, steady eyes with pinpoint catchlights.
    for (ex, gx): (CGFloat, CGFloat) in [(47.2, 45.8), (72.8, 71.4)] {
        fill(ellipse(ex, 52, 4.5, 4.5), eye)
        fill(ellipse(gx, 50.6, 1.35, 1.35), color(0xffffff))
    }

    // Understated warm nose.
    let nose = CGMutablePath()
    nose.move(to: CGPoint(x: 55.2, y: 61.2))
    nose.addCurve(to: CGPoint(x: 64.8, y: 61.2), control1: CGPoint(x: 55.2, y: 58.6), control2: CGPoint(x: 64.8, y: 58.6))
    nose.addCurve(to: CGPoint(x: 60, y: 66.2), control1: CGPoint(x: 64.8, y: 64), control2: CGPoint(x: 62.1, y: 66.2))
    nose.addCurve(to: CGPoint(x: 55.2, y: 61.2), control1: CGPoint(x: 57.9, y: 66.2), control2: CGPoint(x: 55.2, y: 64))
    nose.closeSubpath()
    fill(nose, noseColor)

    // Quiet composed muzzle lines.
    let philtrum = CGMutablePath()
    philtrum.move(to: CGPoint(x: 60, y: 66.2))
    philtrum.addLine(to: CGPoint(x: 60, y: 69.2))
    stroke(philtrum, color(0x25292e, 0.45), width: 1.6)

    let mouth = CGMutablePath()
    mouth.move(to: CGPoint(x: 56.8, y: 69.6))
    mouth.addCurve(to: CGPoint(x: 63.2, y: 69.6), control1: CGPoint(x: 58.8, y: 70.6), control2: CGPoint(x: 61.2, y: 70.6))
    stroke(mouth, color(0x25292e, 0.4), width: 1.5)

    // Honey-brass folio pocket across the bottom.
    let pocketTop = CGMutablePath()
    pocketTop.move(to: CGPoint(x: 0, y: 76))
    pocketTop.addLine(to: CGPoint(x: 41, y: 76))
    pocketTop.addCurve(to: CGPoint(x: 60, y: 82.5), control1: CGPoint(x: 46, y: 76), control2: CGPoint(x: 48, y: 82.5))
    pocketTop.addCurve(to: CGPoint(x: 79, y: 76), control1: CGPoint(x: 72, y: 82.5), control2: CGPoint(x: 74, y: 76))
    pocketTop.addLine(to: CGPoint(x: 120, y: 76))

    let pocket = pocketTop.mutableCopy()!
    pocket.addLine(to: CGPoint(x: 120, y: 120))
    pocket.addLine(to: CGPoint(x: 0, y: 120))
    pocket.closeSubpath()
    gradient(pocket, [color(0xd99a38), color(0xb37820)], from: CGPoint(x: 0, y: 76), to: CGPoint(x: 0, y: 120))
    stroke(pocketTop, color(0xfff9eb, 0.38), width: 1.6, cap: .butt)

    // Crisp four-point helper spark centered on the folio.
    let spark = CGMutablePath()
    spark.move(to: CGPoint(x: 60, y: 90))
    spark.addCurve(to: CGPoint(x: 67.8, y: 97.8), control1: CGPoint(x: 60, y: 96.2), control2: CGPoint(x: 61.6, y: 97.8))
    spark.addCurve(to: CGPoint(x: 60, y: 105.6), control1: CGPoint(x: 61.6, y: 97.8), control2: CGPoint(x: 60, y: 99.4))
    spark.addCurve(to: CGPoint(x: 52.2, y: 97.8), control1: CGPoint(x: 60, y: 99.4), control2: CGPoint(x: 58.4, y: 97.8))
    spark.addCurve(to: CGPoint(x: 60, y: 90), control1: CGPoint(x: 58.4, y: 97.8), control2: CGPoint(x: 60, y: 96.2))
    spark.closeSubpath()
    fill(spark, color(0xfffdf5))

    // Steady paws resting on the folio edge.
    for px in [CGFloat(21), CGFloat(83.5)] {
        let paw = rounded(px, 71.5, 15.5, 8.5, 4.25)
        fill(paw, faceCream)
        stroke(paw, color(0x25292e, 0.18), width: 1)
    }

    ctx.restoreGState()

    if !fullBleed {
        // Subtle inner rim highlight around the macOS squircle.
        stroke(shape, color(0xffffff, 0.2), width: 12, cap: .butt)
    }

    ctx.restoreGState()
}

func png(px: Int, fullBleed: Bool = false) -> Data {
    let ctx = CGContext(
        data: nil, width: px, height: px, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.interpolationQuality = .high
    draw(ctx, px: px, fullBleed: fullBleed)
    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    return rep.representation(using: .png, properties: [:])!
}

let args = Array(CommandLine.arguments.dropFirst())
if args.first == "--pwa", args.count == 2 {
    let out = URL(fileURLWithPath: args[1], isDirectory: true)
    try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
    for (name, size) in [("apple-touch-icon.png", 180), ("icon-192.png", 192), ("icon-512.png", 512)] {
        try png(px: size, fullBleed: true).write(to: out.appendingPathComponent(name))
    }
} else if args.count == 1 {
    let out = URL(fileURLWithPath: args[0], isDirectory: true)
    try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
    for size in [16, 32, 128, 256, 512] {
        try png(px: size).write(to: out.appendingPathComponent("icon_\(size)x\(size).png"))
        try png(px: size * 2).write(to: out.appendingPathComponent("icon_\(size)x\(size)@2x.png"))
    }
} else {
    fputs("Usage: swift scripts/osom-icon.swift [--pwa] OUT_DIR\n", stderr)
    exit(2)
}
