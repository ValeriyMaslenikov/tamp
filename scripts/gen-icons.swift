// Generates app + tray icons. Run from repo root: swift scripts/gen-icons.swift
// - src-tauri/icons/trayicon.png: monochrome menu-bar template icon (44px, 22pt@2x)
// - /tmp/tamp-appicon.png: 1024px source icon, fed to `bun tauri icon`
import AppKit

func symbolImage(_ name: String, pointSize: CGFloat, weight: NSFont.Weight) -> NSImage {
    guard let base = NSImage(systemSymbolName: name, accessibilityDescription: nil),
          let img = base.withSymbolConfiguration(.init(pointSize: pointSize, weight: weight))
    else { fatalError("missing SF Symbol \(name)") }
    return img
}

func renderPNG(pixels: Int, points: CGFloat, to path: String, draw: (NSRect) -> Void) {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: points, height: points)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    draw(NSRect(x: 0, y: 0, width: points, height: points))
    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!
        .write(to: URL(fileURLWithPath: path))
    print("wrote \(path)")
}

// Tray template icon: inward-compress arrows, black (template = recolored by macOS).
let traySymbol = symbolImage("arrow.down.right.and.arrow.up.left", pointSize: 15, weight: .semibold)
renderPNG(pixels: 44, points: 22, to: "src-tauri/icons/trayicon.png") { rect in
    traySymbol.draw(in: rect.insetBy(dx: 2, dy: 2))
}

// App icon: violet gradient rounded square + white compress glyph.
let appSymbol = symbolImage("arrow.down.right.and.arrow.up.left", pointSize: 440, weight: .bold)
renderPNG(pixels: 1024, points: 1024, to: "/tmp/tamp-appicon.png") { rect in
    // macOS icon grid: content square inset ~10%, corner radius ~22.5% of square
    let square = rect.insetBy(dx: 100, dy: 100)
    let bg = NSBezierPath(roundedRect: square, xRadius: 185, yRadius: 185)
    NSGradient(starting: NSColor(red: 0.61, green: 0.47, blue: 1.0, alpha: 1),
               ending: NSColor(red: 0.36, green: 0.23, blue: 0.85, alpha: 1))!
        .draw(in: bg, angle: -90)

    // White glyph: clip to the symbol's alpha mask, fill white.
    let glyphRect = NSRect(x: 277, y: 277, width: 470, height: 470)
    var proposed = glyphRect
    let mask = appSymbol.cgImage(forProposedRect: &proposed, context: nil, hints: nil)!
    let cg = NSGraphicsContext.current!.cgContext
    cg.saveGState()
    cg.clip(to: glyphRect, mask: mask)
    NSColor.white.setFill()
    glyphRect.fill()
    cg.restoreGState()
}
