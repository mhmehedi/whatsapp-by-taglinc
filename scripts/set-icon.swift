// Usage: swift scripts/set-icon.swift <icon.icns> <file>
import AppKit

let args = CommandLine.arguments
guard args.count == 3, let img = NSImage(contentsOfFile: args[1]) else {
    FileHandle.standardError.write("usage: set-icon.swift <icon.icns> <file>\n".data(using: .utf8)!)
    exit(1)
}
exit(NSWorkspace.shared.setIcon(img, forFile: args[2], options: []) ? 0 : 1)
