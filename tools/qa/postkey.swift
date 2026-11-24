// Posts one key chord to a process as real keyboard events (CGEvent, postToPid):
// modifier keys down, the key down and up, modifier keys up, each with the
// matching modifier flags, so AppKit and winit see the modifiers change like on
// a keyboard. Background input: the app does not need to be in front.
//
//   postkey <pid> <virtual key code> [cmd] [shift] [alt] [ctrl]
//
// Built by tools/qa/shortcut_sweep_mac.py (swiftc, once per run).

import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 3, let pid = pid_t(args[1]), let code = UInt16(args[2]) else {
    FileHandle.standardError.write("usage: postkey <pid> <keycode> [cmd] [shift] [alt] [ctrl]\n".data(using: .utf8)!)
    exit(2)
}
let mods = Set(args.dropFirst(3))
// (modifier key code, flag)
var order: [(UInt16, CGEventFlags)] = []
if mods.contains("ctrl") { order.append((0x3B, .maskControl)) }
if mods.contains("alt") { order.append((0x3A, .maskAlternate)) }
if mods.contains("shift") { order.append((0x38, .maskShift)) }
if mods.contains("cmd") { order.append((0x37, .maskCommand)) }

let source = CGEventSource(stateID: .privateState)
var flags: CGEventFlags = []
func post(_ key: UInt16, _ down: Bool) {
    guard let e = CGEvent(keyboardEventSource: source, virtualKey: key, keyDown: down) else { return }
    e.flags = flags
    e.postToPid(pid)
    usleep(15_000)
}
for (key, flag) in order {
    flags.insert(flag)
    post(key, true)
}
post(code, true)
post(code, false)
for (key, flag) in order.reversed() {
    flags.remove(flag)
    post(key, false)
}
