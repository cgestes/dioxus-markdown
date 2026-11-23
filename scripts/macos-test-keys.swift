// Target only the disposable process launched by the test runner.
import AppKit
import CoreGraphics
import Foundation

let args = Array(CommandLine.arguments.dropFirst())
func windowID(_ pid: Int32) -> Int? {
    guard let rows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { return nil }
    return rows.first(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 })?[kCGWindowNumber as String] as? Int
}
switch args.first {
case "launch" where args.count == 3:
    let config = NSWorkspace.OpenConfiguration()
    config.activates = false
    config.createsNewApplicationInstance = true
    config.arguments = ["--test-window"]
    config.environment = ["MARGIN_REPORT_DIR": args[2]]
    NSWorkspace.shared.openApplication(at: URL(fileURLWithPath: args[1]), configuration: config) { app, error in
        guard let app else {
            FileHandle.standardError.write(Data("\(String(describing: error))\n".utf8))
            exit(1)
        }
        print(app.processIdentifier)
        exit(0)
    }
    RunLoop.main.run()
case "report" where args.count == 2:
    guard let pid = Int32(args[1]), let number = windowID(pid) else { exit(2) }
    // F9 writes the editor's own observation without changing the selection.
    let text = "\u{f70c}"
    for down in [true, false] {
        guard let event = NSEvent.keyEvent(with: down ? .keyDown : .keyUp, location: .zero,
            modifierFlags: [.function], timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: number,
            context: nil, characters: text, charactersIgnoringModifiers: text, isARepeat: false, keyCode: 101)?.cgEvent
        else { exit(1) }
        event.postToPid(pid)
    }
case "window" where args.count == 2:
    guard let pid = Int32(args[1]), let id = windowID(pid) else { exit(1) }
    print(id)
default:
    exit(2)
}
