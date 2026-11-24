// Usage: swift scripts/make-dmg-icon.swift <badge.icns> <out.iconset dir>
// Draws the system disk-image icon with the app icon as a badge, at all iconset sizes.
import AppKit
import UniformTypeIdentifiers

let args = CommandLine.arguments
guard args.count == 3, let badge = NSImage(contentsOfFile: args[1]) else {
    FileHandle.standardError.write("usage: make-dmg-icon.swift <badge.icns> <out.iconset>\n".data(using: .utf8)!)
    exit(1)
}
let base = NSWorkspace.shared.icon(for: .diskImage)
let outDir = URL(fileURLWithPath: args[2])
try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

for pt in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let px = pt * scale
        let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
            bytesPerRow: 0, bitsPerPixel: 0)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        NSGraphicsContext.current?.imageInterpolation = .high
        let s = CGFloat(px)
        base.draw(in: NSRect(x: 0, y: 0, width: s, height: s))
        let b = s * 0.52
        badge.draw(in: NSRect(x: (s - b) / 2, y: s * 0.30, width: b, height: b))
        NSGraphicsContext.restoreGraphicsState()
        let name = scale == 1 ? "icon_\(pt)x\(pt).png" : "icon_\(pt)x\(pt)@2x.png"
        try! rep.representation(using: .png, properties: [:])!.write(to: outDir.appendingPathComponent(name))
    }
}
