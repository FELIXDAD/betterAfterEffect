// End-to-end Chinese IME check for BAE on macOS (Simplified Pinyin).
//
// Needs Accessibility / "post events" permission for the terminal that runs it
// (System Settings > Privacy & Security > Accessibility); without it macOS drops
// the synthetic key presses (`CGPreflightPostEventAccess()` is false).
//
//   swiftc -O crates/bae-ui/tools/ime_driver.swift -o /private/tmp/ime_driver
//   BAE_LANG=zh target/debug/bae &  /private/tmp/ime_driver $! /private/tmp/ime-out
//
// It activates BAE, switches to 简体拼音, opens Keyboard Shortcuts (Cmd+Opt+'),
// types "nihao", commits with Space, then composes "jih" + 2× Backspace + Enter,
// capturing the screen region around the BAE window after each step (the pinyin
// candidate window belongs to the input method, not to BAE). It prints the IME
// windows near BAE, then restores the previous input source and front app.
// Key presses are posted to BAE's pid only, never to the frontmost app.

import AppKit
import Carbon
import CoreGraphics
import Foundation

// usage: ime_driver <bae pid> <output dir>
let pid = pid_t(CommandLine.arguments[1])!
let out = CommandLine.arguments[2]

func source(_ id: String) -> TISInputSource? {
    let filter = [kTISPropertyInputSourceID as String: id] as CFDictionary
    guard let list = TISCreateInputSourceList(filter, false)?.takeRetainedValue() as? [TISInputSource] else { return nil }
    return list.first
}
func currentSourceID() -> String {
    let src = TISCopyCurrentKeyboardInputSource().takeRetainedValue()
    let ptr = TISGetInputSourceProperty(src, kTISPropertyInputSourceID)!
    return Unmanaged<CFString>.fromOpaque(ptr).takeUnretainedValue() as String
}
func windows() -> [[String: Any]] {
    CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
}
func bounds(_ w: [String: Any]) -> CGRect {
    CGRect(dictionaryRepresentation: w[kCGWindowBounds as String] as! CFDictionary)!
}
func key(_ code: CGKeyCode, _ flags: CGEventFlags = []) {
    let src = CGEventSource(stateID: .hidSystemState)
    for down in [true, false] {
        let e = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: down)!
        e.flags = flags
        e.postToPid(pid)
        usleep(30_000)
    }
}
func shot(_ name: String, _ rect: CGRect) {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    p.arguments = ["-x", "-R", "\(Int(rect.minX)),\(Int(rect.minY)),\(Int(rect.width)),\(Int(rect.height))", "\(out)/\(name)"]
    try? FileManager.default.createDirectory(atPath: out, withIntermediateDirectories: true)
    try? p.run(); p.waitUntilExit()
}
func imeWindows(_ win: CGRect) -> [String] {
    windows().compactMap { w in
        let owner = w[kCGWindowOwnerName as String] as? String ?? ""
        let layer = w[kCGWindowLayer as String] as? Int ?? 0
        let wpid = w[kCGWindowOwnerPID as String] as? Int32 ?? 0
        if wpid == pid || layer == 0 { return nil }
        let b = bounds(w)
        guard b.intersects(win.insetBy(dx: -200, dy: -200)), b.width < 900 else { return nil }
        return "\(owner) layer=\(layer) bounds=\(b)"
    }
}

guard let bae = windows().first(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }) else {
    print("no BAE window"); exit(1)
}
let win = bounds(bae)
print("BAE window: \(win)")
let previousApp = NSWorkspace.shared.frontmostApplication
let previousSource = currentSourceID()
print("previous input source: \(previousSource), front app: \(previousApp?.localizedName ?? "?")")

guard CGPreflightPostEventAccess() else {
    print("this terminal may not post key events (grant Accessibility permission)"); exit(2)
}
guard let pinyin = source("com.apple.inputmethod.SCIM.ITABC") else { print("pinyin not enabled"); exit(1) }
NSRunningApplication(processIdentifier: pid)?.activate(options: [])
usleep(600_000)
TISSelectInputSource(pinyin)
usleep(300_000)
print("input source now: \(currentSourceID())")

key(39, [.maskCommand, .maskAlternate])   // Cmd+Opt+' : Keyboard Shortcuts, search focused
usleep(500_000)
for c: CGKeyCode in [45, 34, 4, 0, 31] { key(c) }   // n i h a o
usleep(700_000)
let region = win.insetBy(dx: -40, dy: -40)
shot("ime-1-preedit.png", region)
print("IME windows while composing:"); imeWindows(win).forEach { print("  \($0)") }
key(49)                                    // space: commit first candidate
usleep(500_000)
shot("ime-2-commit.png", region)
// Second round: type then Backspace twice while composing, then Enter (commit raw letters).
for c: CGKeyCode in [38, 34, 4] { key(c) } // j i h
usleep(300_000)
key(51); key(51)                           // backspace x2 → "j"
usleep(300_000)
shot("ime-3-backspace.png", region)
key(36)                                    // Enter: commit "j" as Latin
usleep(400_000)
shot("ime-4-enter.png", region)

if let prev = source(previousSource) { TISSelectInputSource(prev) }
usleep(200_000)
print("input source restored: \(currentSourceID())")
previousApp?.activate(options: [])
