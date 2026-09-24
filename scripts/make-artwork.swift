// Draws the app icon and the DMG window background (original artwork, no SF Symbols: their license excludes app icons).
// From the repository root (the `swift` script runner can't link AppKit with Command Line Tools, so compile it):
//   swiftc -o /tmp/make-artwork scripts/make-artwork.swift && /tmp/make-artwork
// Writes app/Resources/AppIcon.icns, app/Resources/dmg-background.tiff (1x + 2x) and docs/images/app-icon.png (the logo
// for the README). Commit the results. The design is described in docs/ARTWORK.md.
import AppKit

let repo = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let resources = repo.appendingPathComponent("app/Resources")
let work = FileManager.default.temporaryDirectory.appendingPathComponent("vtt-artwork")
try? FileManager.default.removeItem(at: work)
try FileManager.default.createDirectory(at: work, withIntermediateDirectories: true)
try FileManager.default.createDirectory(at: resources, withIntermediateDirectories: true)

// The overlay's colours: blue while transcribing, violet while polishing.
let blue = NSColor(srgbRed: 0.35, green: 0.65, blue: 1.0, alpha: 1)
let violet = NSColor(srgbRed: 0.72, green: 0.56, blue: 1.0, alpha: 1)

/// Renders a `width` × `height` point drawing at `scale` pixels per point into a PNG.
func png(width: Int, height: Int, scale: CGFloat, draw: (CGContext, CGFloat) -> Void) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(CGFloat(width) * scale),
                               pixelsHigh: Int(CGFloat(height) * scale), bitsPerSample: 8, samplesPerPixel: 4,
                               hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: width, height: height)
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = context // maps the points in `rep.size` to its pixels
    draw(context.cgContext, scale)
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

func gradient(_ colors: [NSColor]) -> CGGradient {
    CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB), colors: colors.map(\.cgColor) as CFArray, locations: nil)!
}

// MARK: - App icon (1024 pt canvas, Apple's macOS grid: an 824 pt rounded square with room for the shadow)

func drawIcon(_ ctx: CGContext, _ scale: CGFloat) {
    let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
    let shape = CGPath(roundedRect: tile, cornerWidth: 185, cornerHeight: 185, transform: nil)

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 28, color: NSColor.black.withAlphaComponent(0.35).cgColor)
    ctx.addPath(shape)
    ctx.setFillColor(NSColor.black.cgColor)
    ctx.fillPath()
    ctx.restoreGState()

    ctx.saveGState()
    ctx.addPath(shape)
    ctx.clip()
    ctx.drawLinearGradient(gradient([NSColor(srgbRed: 0.17, green: 0.18, blue: 0.26, alpha: 1),
                                     NSColor(srgbRed: 0.07, green: 0.07, blue: 0.11, alpha: 1)]),
                           start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
    // A soft glow behind the waveform.
    ctx.drawRadialGradient(gradient([blue.withAlphaComponent(0.28), blue.withAlphaComponent(0)]),
                           startCenter: CGPoint(x: 512, y: 580), startRadius: 0,
                           endCenter: CGPoint(x: 512, y: 580), endRadius: 380, options: [])
    ctx.restoreGState()

    // Voice: a waveform of rounded bars, blue to violet.
    let heights: [CGFloat] = [0.26, 0.52, 0.8, 1.0, 0.7, 0.44, 0.24]
    let barWidth: CGFloat = 58, gap: CGFloat = 30, maxHeight: CGFloat = 400, centerY: CGFloat = 590
    let total = CGFloat(heights.count) * barWidth + CGFloat(heights.count - 1) * gap
    let bars = CGMutablePath()
    for (index, fraction) in heights.enumerated() {
        let height = max(barWidth, maxHeight * fraction)
        let x = 512 - total / 2 + CGFloat(index) * (barWidth + gap)
        bars.addRoundedRect(in: CGRect(x: x, y: centerY - height / 2, width: barWidth, height: height),
                            cornerWidth: barWidth / 2, cornerHeight: barWidth / 2)
    }
    ctx.saveGState()
    ctx.addPath(bars)
    ctx.clip()
    ctx.drawLinearGradient(gradient([blue, violet]), start: CGPoint(x: 512 - total / 2, y: centerY),
                           end: CGPoint(x: 512 + total / 2, y: centerY), options: [])
    ctx.restoreGState()

    // Text: two lines, the second one shorter, with a cursor.
    ctx.setFillColor(NSColor.white.withAlphaComponent(0.9).cgColor)
    ctx.addPath(CGPath(roundedRect: CGRect(x: 512 - 230, y: 300, width: 460, height: 38),
                       cornerWidth: 19, cornerHeight: 19, transform: nil))
    ctx.fillPath()
    ctx.setFillColor(NSColor.white.withAlphaComponent(0.55).cgColor)
    ctx.addPath(CGPath(roundedRect: CGRect(x: 512 - 230, y: 232, width: 290, height: 38),
                       cornerWidth: 19, cornerHeight: 19, transform: nil))
    ctx.fillPath()
    ctx.setFillColor(blue.cgColor)
    ctx.addPath(CGPath(roundedRect: CGRect(x: 512 + 84, y: 218, width: 16, height: 66),
                       cornerWidth: 8, cornerHeight: 8, transform: nil))
    ctx.fillPath()
}

let iconset = work.appendingPathComponent("AppIcon.iconset")
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let data = png(width: 1024, height: 1024, scale: CGFloat(points * scale) / 1024) { ctx, s in drawIcon(ctx, s) }
        let name = "icon_\(points)x\(points)\(scale == 2 ? "@2x" : "").png"
        try data.write(to: iconset.appendingPathComponent(name))
    }
}
try png(width: 1024, height: 1024, scale: 1) { ctx, s in drawIcon(ctx, s) }.write(to: work.appendingPathComponent("icon-1024.png"))
try png(width: 1024, height: 1024, scale: 0.5) { ctx, s in drawIcon(ctx, s) }
    .write(to: repo.appendingPathComponent("docs/images/app-icon.png"))

// MARK: - DMG background (660 × 400 pt window: the app on the left, Applications on the right, chevrons between)

func drawBackground(_ ctx: CGContext, _ scale: CGFloat) {
    let size = CGSize(width: 660, height: 400)
    ctx.setFillColor(NSColor(srgbRed: 0.985, green: 0.985, blue: 0.98, alpha: 1).cgColor)
    ctx.fill(CGRect(origin: .zero, size: size))

    // Three chevrons, fading in from the left, centred between the icons (Finder's y grows downwards: icons at y 190).
    let midY = size.height - 190
    for index in 0..<3 {
        let x = 300 + CGFloat(index) * 22
        let alpha = [0.18, 0.4, 0.8][index]
        ctx.setStrokeColor(NSColor(white: 0.25, alpha: alpha).cgColor)
        ctx.setLineWidth(5)
        ctx.setLineCap(.round)
        ctx.setLineJoin(.round)
        ctx.move(to: CGPoint(x: x, y: midY + 20))
        ctx.addLine(to: CGPoint(x: x + 20, y: midY))
        ctx.addLine(to: CGPoint(x: x, y: midY - 20))
        ctx.strokePath()
    }

    let caption = "Drag Voice to Text to Applications" as NSString
    let attributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: 13, weight: .regular),
        .foregroundColor: NSColor(white: 0.45, alpha: 1),
    ]
    let textSize = caption.size(withAttributes: attributes)
    caption.draw(at: CGPoint(x: (size.width - textSize.width) / 2, y: 52), withAttributes: attributes)
}

let background1x = work.appendingPathComponent("background.png")
let background2x = work.appendingPathComponent("background@2x.png")
try png(width: 660, height: 400, scale: 1) { ctx, s in drawBackground(ctx, s) }.write(to: background1x)
try png(width: 660, height: 400, scale: 2) { ctx, s in drawBackground(ctx, s) }.write(to: background2x)

func run(_ tool: String, _ args: [String]) throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: tool)
    process.arguments = args
    try process.run()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else { throw NSError(domain: tool, code: Int(process.terminationStatus)) }
}
try run("/usr/bin/iconutil", ["-c", "icns", iconset.path, "-o", resources.appendingPathComponent("AppIcon.icns").path])
// One TIFF with both resolutions: Finder picks the sharp one on Retina screens.
try run("/usr/bin/tiffutil", ["-cathidpicheck", background1x.path, background2x.path,
                              "-out", resources.appendingPathComponent("dmg-background.tiff").path])
print("Wrote app/Resources/AppIcon.icns, app/Resources/dmg-background.tiff and docs/images/app-icon.png (previews in \(work.path))")
