// Renders playful demo thumbnails for README screenshots (no real recordings).
// Usage: swift scripts/gen-demo-thumbs.swift <outdir>
import AppKit

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "public/mock"
try? FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)

struct Thumb { let emoji: String; let from: NSColor; let to: NSColor }
let thumbs: [Thumb] = [
    .init(emoji: "🐈", from: NSColor(red: 0.95, green: 0.45, blue: 0.30, alpha: 1), to: NSColor(red: 0.55, green: 0.12, blue: 0.35, alpha: 1)),
    .init(emoji: "🎮", from: NSColor(red: 0.35, green: 0.35, blue: 0.95, alpha: 1), to: NSColor(red: 0.12, green: 0.10, blue: 0.45, alpha: 1)),
    .init(emoji: "🦆", from: NSColor(red: 0.20, green: 0.75, blue: 0.65, alpha: 1), to: NSColor(red: 0.05, green: 0.30, blue: 0.40, alpha: 1)),
    .init(emoji: "🚀", from: NSColor(red: 0.55, green: 0.40, blue: 0.95, alpha: 1), to: NSColor(red: 0.20, green: 0.10, blue: 0.50, alpha: 1)),
    .init(emoji: "💥", from: NSColor(red: 0.95, green: 0.70, blue: 0.25, alpha: 1), to: NSColor(red: 0.60, green: 0.20, blue: 0.10, alpha: 1)),
    .init(emoji: "🍕", from: NSColor(red: 0.90, green: 0.30, blue: 0.45, alpha: 1), to: NSColor(red: 0.35, green: 0.08, blue: 0.30, alpha: 1)),
]

for (i, t) in thumbs.enumerated() {
    let w = 320, h = 200
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: w, pixelsHigh: h,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let rect = NSRect(x: 0, y: 0, width: w, height: h)
    NSGradient(starting: t.from, ending: t.to)!.draw(in: rect, angle: -55)
    // faint "play bar" strip at the bottom for a video-ish look
    NSColor(white: 0, alpha: 0.25).setFill()
    NSRect(x: 0, y: 0, width: w, height: 22).fill()
    NSColor(white: 1, alpha: 0.85).setFill()
    NSRect(x: 10, y: 9, width: CGFloat(40 + i * 45), height: 4).fill()
    let attrs: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: 96)]
    let s = NSAttributedString(string: t.emoji, attributes: attrs)
    let size = s.size()
    s.draw(at: NSPoint(x: (CGFloat(w) - size.width) / 2, y: (CGFloat(h) - size.height) / 2 + 8))
    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .jpeg, properties: [.compressionFactor: 0.92])!
        .write(to: URL(fileURLWithPath: "\(outDir)/thumb\(i + 1).jpg"))
    print("wrote \(outDir)/thumb\(i + 1).jpg")
}
