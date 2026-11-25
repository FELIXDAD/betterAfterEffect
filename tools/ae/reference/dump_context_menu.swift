// Dump the context menu After Effects has open near screen point (x, y) as JSON:
// `swiftc -O dump_context_menu.swift -o dump_ctx && ./dump_ctx <x> <y>`. AE does not list
// open menus among its accessibility children, so the menu is found by hit-testing
// points around (x, y). Needs the Accessibility permission.

import Cocoa
import ApplicationServices
func a(_ e: AXUIElement, _ k: String) -> AnyObject? { var v: AnyObject?; return AXUIElementCopyAttributeValue(e, k as CFString, &v) == .success ? v : nil }
func dump(_ menu: AXUIElement, _ depth: Int) -> [[String: Any]] {
  guard depth < 6, let kids = a(menu, kAXChildrenAttribute) as? [AXUIElement] else { return [] }
  var out: [[String: Any]] = []
  for it in kids {
    let t = (a(it, kAXTitleAttribute) as? String) ?? ""
    if t.isEmpty { out.append(["type": "separator"]); continue }
    var e: [String: Any] = ["title": t]
    if let en = a(it, kAXEnabledAttribute) as? Bool, !en { e["enabled"] = false }
    if let m = a(it, kAXMenuItemMarkCharAttribute) as? String, !m.isEmpty { e["checked"] = true }
    if let c = a(it, kAXMenuItemCmdCharAttribute) as? String, !c.isEmpty { e["cmdChar"] = c; e["cmdModifiers"] = (a(it, kAXMenuItemCmdModifiersAttribute) as? Int) ?? 0 }
    if let sub = (a(it, kAXChildrenAttribute) as? [AXUIElement])?.first, (a(sub, kAXRoleAttribute) as? String) == (kAXMenuRole as String) {
      e["type"] = "submenu"; e["items"] = dump(sub, depth + 1)
    } else { e["type"] = "item" }
    out.append(e)
  }
  return out
}
let args = CommandLine.arguments
let cx = Float(args[1])!, cy = Float(args[2])!
let sys = AXUIElementCreateSystemWide()
var found: AXUIElement? = nil
outer: for dy in stride(from: Float(10), through: 700, by: 18) { for sign: Float in [1, -1] { for dx: Float in [40, 120, -60, -160] {
  var el: AXUIElement?
  if AXUIElementCopyElementAtPosition(sys, cx + dx, cy + sign * dy, &el) == .success, var e = el {
    for _ in 0..<4 {
      if (a(e, kAXRoleAttribute) as? String) == (kAXMenuRole as String) { found = e; break outer }
      guard let p = a(e, kAXParentAttribute) else { break }
      e = p as! AXUIElement
    }
  }
} } }
guard let menu = found else { print("[]"); exit(3) }
let data = try! JSONSerialization.data(withJSONObject: dump(menu, 0), options: [.prettyPrinted, .sortedKeys])
print(String(data: data, encoding: .utf8)!)
