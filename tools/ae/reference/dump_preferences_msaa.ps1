param(
    [int64]$DialogHwnd = 0,
    [string]$OutPath = "",
    [string]$Desktop = "Default",
    [switch]$ProbeLinkages
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -AssemblyName Accessibility

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using Accessibility;

public static class AePrefMsaaTool {
    public const uint DESKTOP_ALL = 0x000F01FF;

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr OpenDesktop(string lpszDesktop, uint dwFlags, bool fInherit, uint dwDesiredAccess);
    [DllImport("user32.dll")]
    public static extern bool SetThreadDesktop(IntPtr hDesktop);
    [DllImport("user32.dll")]
    public static extern bool CloseDesktop(IntPtr hDesktop);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern bool EnumChildWindows(IntPtr hWndParent, EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);
    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr hWnd);

    [DllImport("oleacc.dll")]
    public static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint dwId, ref Guid riid, [MarshalAs(UnmanagedType.Interface)] out object ppvObject);
    [DllImport("oleacc.dll")]
    public static extern int AccessibleChildren(IAccessible paccContainer, int iChildStart, int cChildren, [Out] object[] rgvarChildren, out int pcObtained);

    static string SafeStr(Func<string> f) { try { return f() ?? ""; } catch { return ""; } }
    static int SafeInt(Func<int> f) { try { return f(); } catch { return 0; } }

    static string ReadClass(IntPtr h) {
        var sb = new StringBuilder(256);
        GetClassName(h, sb, sb.Capacity);
        return sb.ToString();
    }

    static string ReadTitle(IntPtr h) {
        var sb = new StringBuilder(256);
        GetWindowText(h, sb, sb.Capacity);
        return sb.ToString();
    }

    static T RunStaOnDesktop<T>(string desktopName, Func<T> action) {
        T output = default(T);
        Exception err = null;
        var t = new Thread(() => {
            IntPtr hDesk = string.IsNullOrEmpty(desktopName) ? IntPtr.Zero : OpenDesktop(desktopName, 0, false, DESKTOP_ALL);
            try {
                if (hDesk != IntPtr.Zero) SetThreadDesktop(hDesk);
                output = action();
            } catch (Exception ex) {
                err = ex;
            } finally {
                if (hDesk != IntPtr.Zero) CloseDesktop(hDesk);
            }
        });
        t.SetApartmentState(ApartmentState.STA);
        t.Start();
        t.Join();
        if (err != null) throw err;
        return output;
    }

    public static long FindPreferencesDialog(uint pid, string desktopName) {
        return RunStaOnDesktop(desktopName, () => {
            IntPtr found = IntPtr.Zero;
            EnumWindows((h, l) => {
                uint wPid;
                GetWindowThreadProcessId(h, out wPid);
                if (wPid == pid && IsWindowVisible(h) && ReadClass(h) == "#32770") {
                    found = h;
                    return false;
                }
                return true;
            }, IntPtr.Zero);
            return found.ToInt64();
        });
    }

    static IAccessible GetRootAccessible(IntPtr dlgHwnd) {
        IntPtr viewHwnd = IntPtr.Zero;
        EnumChildWindows(dlgHwnd, (c, l) => {
            if (IsWindowVisible(c) && viewHwnd == IntPtr.Zero) { viewHwnd = c; return false; }
            return true;
        }, IntPtr.Zero);
        Guid iid = new Guid("618736E0-3C3D-11CF-810C-00AA00389B71");
        object accObj;
        int hr = AccessibleObjectFromWindow(viewHwnd != IntPtr.Zero ? viewHwnd : dlgHwnd, 0xFFFFFFFC, ref iid, out accObj);
        if (hr != 0 || !(accObj is IAccessible)) {
            throw new Exception("AccessibleObjectFromWindow failed HR=" + hr);
        }
        return (IAccessible)accObj;
    }

    static Dictionary<string, object> MakeRect(int x, int y, int w, int h, RECT dlgRect) {
        return new Dictionary<string, object> {
            { "x", x - dlgRect.Left }, { "y", y - dlgRect.Top },
            { "w", w }, { "h", h }
        };
    }

    static string RoleName(int role) {
        switch (role) {
            case 10: return "Client";
            case 20: return "Grouping";
            case 30: return "Link";
            case 33: return "List";
            case 34: return "ListItem";
            case 35: return "Outline";
            case 36: return "OutlineItem";
            case 41: return "StaticText";
            case 42: return "Text";
            case 43: return "PushButton";
            case 44: return "CheckButton";
            case 45: return "RadioButton";
            case 46: return "ComboBox";
            case 51: return "Slider";
            case 52: return "SpinButton";
            case 0: return "Custom";
            default: return "Role_" + role;
        }
    }

    static Dictionary<string, object> DumpNode(IAccessible acc, int depth, RECT dlgRect, HashSet<string> visited) {
        if (depth > 8) return null;
        string name = SafeStr(() => acc.get_accName(0));
        string val = SafeStr(() => acc.get_accValue(0));
        int role = SafeInt(() => Convert.ToInt32(acc.get_accRole(0)));
        int state = SafeInt(() => Convert.ToInt32(acc.get_accState(0)));
        int childCount = SafeInt(() => acc.accChildCount);
        int x = 0, y = 0, w = 0, h = 0;
        try { acc.accLocation(out x, out y, out w, out h, 0); } catch {}

        string sig = string.Format("{0}|{1}|{2}|{3}|{4}|{5}|{6}", role, name, x, y, w, h, childCount);
        if (depth > 0 && name == "OS_ViewContainer" && visited.Contains(sig)) return null;
        visited.Add(sig);

        var d = new Dictionary<string, object> {
            { "role", role },
            { "roleName", RoleName(role) },
            { "name", name },
            { "state", state },
            { "stateHex", string.Format("0x{0:X}", state) },
            { "visible", (state & 0x8000) == 0 },
            { "unavailable", (state & 0x1) != 0 },
            { "focusable", (state & 0x100000) != 0 },
            { "rect", MakeRect(x, y, w, h, dlgRect) }
        };
        if ((state & 0x10) != 0) d["checked"] = true;
        if ((state & 0x2) != 0) d["selected"] = true;
        if (!string.IsNullOrEmpty(val)) d["value"] = val;

        if (childCount > 0 && childCount <= 500) {
            object[] children = new object[childCount];
            int obtained = 0;
            AccessibleChildren(acc, 0, childCount, children, out obtained);
            var chList = new List<Dictionary<string, object>>();
            for (int i = 0; i < obtained; i++) {
                var ch = children[i];
                if (ch is IAccessible) {
                    var cn = DumpNode((IAccessible)ch, depth + 1, dlgRect, visited);
                    if (cn != null) chList.Add(cn);
                } else if (ch != null) {
                    string cname = SafeStr(() => acc.get_accName(ch));
                    int crole = SafeInt(() => Convert.ToInt32(acc.get_accRole(ch)));
                    int cst = SafeInt(() => Convert.ToInt32(acc.get_accState(ch)));
                    var ci = new Dictionary<string, object> {
                        { "childId", ch },
                        { "role", crole },
                        { "roleName", RoleName(crole) },
                        { "name", cname }
                    };
                    if ((cst & 0x2) != 0) ci["selected"] = true;
                    chList.Add(ci);
                }
            }
            if (chList.Count > 0) d["children"] = chList;
        }
        return d;
    }

    public static Dictionary<string, object> DumpAllPages(long dialogHwndLong, string desktopName) {
        return RunStaOnDesktop(desktopName, () => {
            IntPtr dlgHwnd = new IntPtr(dialogHwndLong);
            RECT dlgRect;
            GetWindowRect(dlgHwnd, out dlgRect);
            uint dpi = 96;
            try { dpi = GetDpiForWindow(dlgHwnd); } catch {}

            IAccessible root = GetRootAccessible(dlgHwnd);
            int rootCount = root.accChildCount;
            object[] rootChildren = new object[rootCount];
            int obtained = 0;
            AccessibleChildren(root, 0, rootCount, rootChildren, out obtained);

            IAccessible listBox = null;
            var sidebarItems = new List<string>();
            for (int i = 0; i < obtained; i++) {
                var ch = rootChildren[i] as IAccessible;
                if (ch == null) continue;
                if (SafeInt(() => Convert.ToInt32(ch.get_accRole(0))) == 33 && SafeStr(() => ch.get_accName(0)) == "UI_ListBox") {
                    listBox = ch;
                    int lc = ch.accChildCount;
                    object[] lch = new object[lc];
                    int lobt = 0;
                    AccessibleChildren(ch, 0, lc, lch, out lobt);
                    for (int j = 0; j < lobt; j++) {
                        var li = lch[j] as IAccessible;
                        if (li != null) sidebarItems.Add(SafeStr(() => li.get_accName(0)));
                    }
                    break;
                }
            }

            var pages = new List<Dictionary<string, object>>();
            int totalPages = sidebarItems.Count;
            object[] listItemObjs = new object[totalPages];
            int listObt = 0;
            if (listBox != null) AccessibleChildren(listBox, 0, totalPages, listItemObjs, out listObt);

            for (int p = 0; p < listObt; p++) {
                var li = listItemObjs[p] as IAccessible;
                if (li != null) {
                    try { li.accSelect(3, 0); } catch {}
                    try { li.accDoDefaultAction(0); } catch {}
                    Thread.Sleep(250);
                }
                int rc2 = root.accChildCount;
                object[] rch2 = new object[rc2];
                int robt2 = 0;
                AccessibleChildren(root, 0, rc2, rch2, out robt2);

                var visibleNodes = new List<Dictionary<string, object>>();
                var dialogBtns = new List<Dictionary<string, object>>();
                for (int i = 0; i < robt2; i++) {
                    var ch = rch2[i] as IAccessible;
                    if (ch == null) continue;
                    string cname = SafeStr(() => ch.get_accName(0));
                    int crole = SafeInt(() => Convert.ToInt32(ch.get_accRole(0)));
                    int cst = SafeInt(() => Convert.ToInt32(ch.get_accState(0)));
                    if (crole == 33 && cname == "UI_ListBox") continue;
                    if (crole == 43 && (i >= robt2 - 4)) {
                        int x = 0, y = 0, w = 0, h = 0;
                        try { ch.accLocation(out x, out y, out w, out h, 0); } catch {}
                        dialogBtns.Add(new Dictionary<string, object> {
                            { "name", cname },
                            { "enabled", (cst & 0x1) == 0 && (cst & 0x100000) != 0 },
                            { "rect", MakeRect(x, y, w, h, dlgRect) }
                        });
                        continue;
                    }
                    if ((cst & 0x8000) != 0) continue;
                    var visited = new HashSet<string>();
                    var node = DumpNode(ch, 0, dlgRect, visited);
                    if (node != null) visibleNodes.Add(node);
                }

                pages.Add(new Dictionary<string, object> {
                    { "index", p + 1 },
                    { "sidebarLabel", sidebarItems[p] },
                    { "dialogButtons", dialogBtns },
                    { "controls", visibleNodes }
                });
            }

            return new Dictionary<string, object> {
                { "title", ReadTitle(dlgHwnd) },
                { "dpi", dpi },
                { "dialogRect", new Dictionary<string, object> {
                    { "w", dlgRect.Right - dlgRect.Left },
                    { "h", dlgRect.Bottom - dlgRect.Top }
                }},
                { "sidebar", sidebarItems },
                { "pages", pages }
            };
        });
    }
}
"@

$proc = Get-Process -Name "AfterFX" -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { throw "AfterFX process is not running." }

if ($DialogHwnd -eq 0) {
    $DialogHwnd = [AePrefMsaaTool]::FindPreferencesDialog([uint32]$proc.Id, $Desktop)
    if ($DialogHwnd -eq 0) { throw "Could not find visible #32770 Preferences dialog for AfterFX." }
}

$res = [AePrefMsaaTool]::DumpAllPages($DialogHwnd, $Desktop)
$json = $res | ConvertTo-Json -Depth 25
if ($OutPath) {
    $dir = Split-Path -Parent $OutPath
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    [System.IO.File]::WriteAllText($OutPath, $json, [System.Text.UTF8Encoding]::new($false))
    Write-Output "Wrote $OutPath ($($json.Length) chars)"
} else {
    Write-Output $json
}
