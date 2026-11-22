#!/usr/bin/env swift
// Dump the live macOS After Effects AXMenuBar tree as JSON to stdout.
// Requires macOS Accessibility permission for the invoking terminal/host.

import Cocoa
import ApplicationServices

func getAttr(_ elem: AXUIElement, _ attr: String) -> AnyObject? {
    var val: AnyObject?
    let err = AXUIElementCopyAttributeValue(elem, attr as CFString, &val)
    return err == .success ? val : nil
}

func dumpMenu(_ menu: AXUIElement, depth: Int) -> [[String: Any]] {
    guard depth < 8, let children = getAttr(menu, kAXChildrenAttribute) as? [AXUIElement] else {
        return []
    }
    var result: [[String: Any]] = []
    for item in children {
        let title = (getAttr(item, kAXTitleAttribute) as? String) ?? ""
        if title.isEmpty {
            result.append(["type": "separator"])
            continue
        }
        var entry: [String: Any] = ["title": title]
        if let cmdChar = getAttr(item, kAXMenuItemCmdCharAttribute) as? String, !cmdChar.isEmpty {
            entry["cmdChar"] = cmdChar
        }
        if let cmdMods = getAttr(item, kAXMenuItemCmdModifiersAttribute) as? Int {
            entry["cmdModifiers"] = cmdMods
        }
        if let cmdGlyph = getAttr(item, kAXMenuItemCmdGlyphAttribute) as? Int, cmdGlyph != 0 {
            entry["cmdGlyph"] = cmdGlyph
        }
        if let cmdVKey = getAttr(item, kAXMenuItemCmdVirtualKeyAttribute) as? Int, cmdVKey != 0 {
            entry["cmdVirtualKey"] = cmdVKey
        }
        if let subChildren = getAttr(item, kAXChildrenAttribute) as? [AXUIElement], let subMenu = subChildren.first {
            let role = (getAttr(subMenu, kAXRoleAttribute) as? String) ?? ""
            if role == kAXMenuRole as String {
                entry["type"] = "submenu"
                if title == "Services" || title == "服务" {
                    entry["items"] = [["type": "item", "title": "<redacted:macOS Services>"]]
                } else {
                    entry["items"] = dumpMenu(subMenu, depth: depth + 1)
                }
            } else {
                entry["type"] = "item"
            }
        } else {
            entry["type"] = "item"
        }
        result.append(entry)
    }
    return result
}

let apps = NSRunningApplication.runningApplications(withBundleIdentifier: "com.adobe.AfterEffects.application")
guard let ae = apps.first else {
    fputs("Error: After Effects is not running.\n", stderr)
    exit(1)
}

let axApp = AXUIElementCreateApplication(ae.processIdentifier)
guard let menuBarObj = getAttr(axApp, kAXMenuBarAttribute) else {
    fputs("Error: Failed to read AXMenuBar (Accessibility permission may be missing).\n", stderr)
    exit(2)
}
let menuBar = menuBarObj as! AXUIElement
guard let barItems = getAttr(menuBar, kAXChildrenAttribute) as? [AXUIElement] else {
    fputs("Error: Failed to read AXMenuBar children.\n", stderr)
    exit(2)
}

var menus: [[String: Any]] = []
for barItem in barItems {
    let title = (getAttr(barItem, kAXTitleAttribute) as? String) ?? ""
    if title == "Apple" || title.isEmpty {
        continue
    }
    var top: [String: Any] = ["type": "submenu", "title": title]
    if let subChildren = getAttr(barItem, kAXChildrenAttribute) as? [AXUIElement], let subMenu = subChildren.first {
        top["items"] = dumpMenu(subMenu, depth: 1)
    } else {
        top["items"] = []
    }
    menus.append(top)
}

let data = try JSONSerialization.data(withJSONObject: menus, options: [.prettyPrinted])
FileHandle.standardOutput.write(data)
