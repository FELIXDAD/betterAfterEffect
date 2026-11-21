param(
    [string]$Mode = "windows", # windows | tree | command | win32
    [int64]$Hwnd = 0,
    [int64]$CommandId = 0,
    [string]$OutPath = "",
    [int]$MaxDepth = 16,
    [switch]$IncludeHidden,
    [string]$Desktop = "Default"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class AeWin32Helper {
    public const uint WM_COMMAND = 0x0111;
    public const uint WM_CLOSE = 0x0010;
    public const uint CB_GETCOUNT = 0x0146;
    public const uint CB_GETLBTEXT = 0x0148;
    public const uint CB_GETLBTEXTLEN = 0x0149;
    public const uint CB_GETCURSEL = 0x0147;
    public const uint GW_OWNER = 4;
    public const uint DESKTOP_ALL = 0x000F01FF;

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

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

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool IsWindowEnabled(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern IntPtr GetWindow(IntPtr hWnd, uint uCmd);

    [DllImport("user32.dll")]
    public static extern IntPtr GetMenu(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern int GetMenuItemCount(IntPtr hMenu);

    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr SendMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr SendMessageText(IntPtr hWnd, uint Msg, IntPtr wParam, StringBuilder lParam);

    [DllImport("user32.dll")]
    public static extern int GetDlgCtrlID(IntPtr hWnd);

    public static T RunOnDesktop<T>(string desktopName, Func<T> action) {
        if (string.IsNullOrEmpty(desktopName)) {
            return action();
        }
        T output = default(T);
        Exception err = null;
        var t = new Thread(() => {
            IntPtr hDesk = OpenDesktop(desktopName, 0, false, DESKTOP_ALL);
            try {
                if (hDesk != IntPtr.Zero) {
                    SetThreadDesktop(hDesk);
                }
                output = action();
            } catch (Exception ex) {
                err = ex;
            } finally {
                if (hDesk != IntPtr.Zero) {
                    CloseDesktop(hDesk);
                }
            }
        });
        t.SetApartmentState(ApartmentState.MTA);
        t.Start();
        t.Join();
        if (err != null) throw err;
        return output;
    }

    public static string ReadTitle(IntPtr hWnd) {
        var sb = new StringBuilder(512);
        GetWindowText(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }

    public static string ReadClass(IntPtr hWnd) {
        var sb = new StringBuilder(256);
        GetClassName(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }

    public static List<Dictionary<string, object>> GetProcessTopWindowInfos(uint pid, bool includeHidden, string desktopName) {
        return RunOnDesktop(desktopName, () => {
            var res = new List<Dictionary<string, object>>();
            EnumWindows((hWnd, lParam) => {
                uint wPid;
                GetWindowThreadProcessId(hWnd, out wPid);
                if (wPid == pid && (includeHidden || IsWindowVisible(hWnd))) {
                    RECT r;
                    GetWindowRect(hWnd, out r);
                    IntPtr hMenu = GetMenu(hWnd);
                    int menuCount = hMenu != IntPtr.Zero ? GetMenuItemCount(hMenu) : 0;
                    IntPtr owner = GetWindow(hWnd, GW_OWNER);
                    res.Add(new Dictionary<string, object> {
                        { "hwnd", hWnd.ToInt64() },
                        { "title", ReadTitle(hWnd) },
                        { "className", ReadClass(hWnd) },
                        { "visible", IsWindowVisible(hWnd) },
                        { "enabled", IsWindowEnabled(hWnd) },
                        { "ownerHwnd", owner.ToInt64() },
                        { "menuCount", menuCount },
                        { "rect", new Dictionary<string, object> {
                            { "x", r.Left },
                            { "y", r.Top },
                            { "w", r.Right - r.Left },
                            { "h", r.Bottom - r.Top }
                        }}
                    });
                }
                return true;
            }, IntPtr.Zero);
            return res;
        });
    }

    public static List<Dictionary<string, object>> GetChildWindowInfos(IntPtr parent, string desktopName) {
        return RunOnDesktop(desktopName, () => {
            RECT parentRect;
            GetWindowRect(parent, out parentRect);
            var res = new List<Dictionary<string, object>>();
            EnumChildWindows(parent, (c, lParam) => {
                RECT r;
                GetWindowRect(c, out r);
                string cls = ReadClass(c);
                var item = new Dictionary<string, object> {
                    { "hwnd", c.ToInt64() },
                    { "ctrlId", GetDlgCtrlID(c) },
                    { "className", cls },
                    { "text", ReadTitle(c) },
                    { "visible", IsWindowVisible(c) },
                    { "enabled", IsWindowEnabled(c) },
                    { "rect", new Dictionary<string, object> {
                        { "x", r.Left - parentRect.Left },
                        { "y", r.Top - parentRect.Top },
                        { "w", r.Right - r.Left },
                        { "h", r.Bottom - r.Top }
                    }}
                };
                if (cls.IndexOf("ComboBox", StringComparison.OrdinalIgnoreCase) >= 0) {
                    var cbItems = ReadWin32ComboBoxItems(c);
                    if (cbItems.Count > 0) item["comboItems"] = cbItems;
                }
                res.Add(item);
                return true;
            }, IntPtr.Zero);
            return res;
        });
    }

    public static bool PostCommandOnDesktop(IntPtr hWnd, int commandId, string desktopName) {
        return RunOnDesktop(desktopName, () => PostMessage(hWnd, WM_COMMAND, (IntPtr)commandId, IntPtr.Zero));
    }

    public static List<string> ReadWin32ComboBoxItems(IntPtr hWnd) {
        var items = new List<string>();
        int count = (int)SendMessage(hWnd, CB_GETCOUNT, IntPtr.Zero, IntPtr.Zero);
        if (count <= 0 || count > 500) return items;
        for (int i = 0; i < count; i++) {
            int len = (int)SendMessage(hWnd, CB_GETLBTEXTLEN, (IntPtr)i, IntPtr.Zero);
            if (len >= 0 && len < 4096) {
                var sb = new StringBuilder(len + 2);
                SendMessageText(hWnd, CB_GETLBTEXT, (IntPtr)i, sb);
                items.Add(sb.ToString());
            }
        }
        return items;
    }
}
"@

function Get-AeProcess {
    $proc = Get-Process -Name "AfterFX" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $proc) {
        throw "AfterFX process is not running."
    }
    return $proc
}

function Get-WindowInfoList {
    param([uint32]$ProcessId, [bool]$IncludeHiddenFlag, [string]$DesktopName)
    return ,@([AeWin32Helper]::GetProcessTopWindowInfos($ProcessId, $IncludeHiddenFlag, $DesktopName))
}

function Dump-UiaElement {
    param(
        [System.Windows.Automation.AutomationElement]$Elem,
        [int]$Depth,
        [int]$LimitDepth,
        [double]$OriginX,
        [double]$OriginY
    )
    if ($null -eq $Elem -or $Depth -gt $LimitDepth) { return $null }

    $cur = $Elem.Current
    $ct = $cur.ControlType.ProgrammaticName -replace '^ControlType\.', ''
    $br = $cur.BoundingRectangle
    $rectObj = $null
    if (-not [double]::IsInfinity($br.X) -and -not [double]::IsNaN($br.X)) {
        $rectObj = [ordered]@{
            x = [math]::Round($br.X - $OriginX, 1)
            y = [math]::Round($br.Y - $OriginY, 1)
            w = [math]::Round($br.Width, 1)
            h = [math]::Round($br.Height, 1)
            sx = [math]::Round($br.X, 1)
            sy = [math]::Round($br.Y, 1)
        }
    }

    $node = [ordered]@{
        type = $ct
        name = $cur.Name
    }
    if ($cur.AutomationId) { $node["automationId"] = $cur.AutomationId }
    if ($cur.ClassName) { $node["className"] = $cur.ClassName }
    if (-not $cur.IsEnabled) { $node["enabled"] = $false }
    if ($cur.IsOffscreen) { $node["offscreen"] = $true }
    if ($cur.HelpText) { $node["helpText"] = $cur.HelpText }
    if ($rectObj) { $node["rect"] = $rectObj }

    $patObj = $null
    try {
        if ($Elem.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$patObj)) {
            $node["toggleState"] = $patObj.Current.ToggleState.ToString()
        }
    } catch {}
    try {
        if ($Elem.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$patObj)) {
            $node["value"] = $patObj.Current.Value
            if ($patObj.Current.IsReadOnly) { $node["readOnly"] = $true }
        }
    } catch {}
    try {
        if ($Elem.TryGetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern, [ref]$patObj)) {
            $node["range"] = [ordered]@{
                value = $patObj.Current.Value
                min = $patObj.Current.Minimum
                max = $patObj.Current.Maximum
                smallChange = $patObj.Current.SmallChange
                largeChange = $patObj.Current.LargeChange
            }
        }
    } catch {}
    try {
        if ($Elem.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$patObj)) {
            $node["selected"] = [bool]$patObj.Current.IsSelected
        }
    } catch {}
    try {
        if ($Elem.TryGetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern, [ref]$patObj)) {
            $sel = $patObj.Current.GetSelection()
            if ($sel -and $sel.Length -gt 0) {
                $node["selection"] = @($sel | ForEach-Object { $_.Current.Name })
            }
        }
    } catch {}
    try {
        if ($Elem.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$patObj)) {
            $node["expandState"] = $patObj.Current.ExpandCollapseState.ToString()
        }
    } catch {}

    $walker = [System.Windows.Automation.TreeWalker]::RawViewWalker
    $children = New-Object System.Collections.ArrayList
    $child = $null
    try { $child = $walker.GetFirstChild($Elem) } catch {}
    while ($null -ne $child) {
        try {
            $cNode = Dump-UiaElement -Elem $child -Depth ($Depth + 1) -LimitDepth $LimitDepth -OriginX $OriginX -OriginY $OriginY
            if ($null -ne $cNode) {
                [void]$children.Add($cNode)
            }
        } catch {}
        try { $child = $walker.GetNextSibling($child) } catch { break }
    }
    if ($children.Count -gt 0) {
        $node["children"] = @($children)
    }
    return $node
}

function Dump-Win32Children {
    param([IntPtr]$ParentHwnd, [string]$DesktopName)
    return ,@([AeWin32Helper]::GetChildWindowInfos($ParentHwnd, $DesktopName))
}

$proc = Get-AeProcess
$result = $null

if ($Mode -eq "windows") {
    $result = [ordered]@{
        pid = $proc.Id
        desktop = $Desktop
        windows = @(Get-WindowInfoList -ProcessId ([uint32]$proc.Id) -IncludeHiddenFlag ([bool]$IncludeHidden) -DesktopName $Desktop)
    }
}
elseif ($Mode -eq "command") {
    if ($Hwnd -eq 0) {
        $wins = Get-WindowInfoList -ProcessId ([uint32]$proc.Id) -IncludeHiddenFlag $false -DesktopName $Desktop
        $mainWin = $wins | Where-Object { $_.menuCount -gt 0 } | Select-Object -First 1
        if (-not $mainWin) { throw "Could not find AE main window with menu." }
        $Hwnd = $mainWin.hwnd
    }
    $ok = [AeWin32Helper]::PostCommandOnDesktop([IntPtr]$Hwnd, [int]$CommandId, $Desktop)
    $result = [ordered]@{
        hwnd = $Hwnd
        commandId = $CommandId
        posted = $ok
    }
}
elseif ($Mode -eq "tree") {
    if ($Hwnd -eq 0) {
        $Hwnd = $proc.MainWindowHandle.ToInt64()
    }
    $elem = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
    if ($null -eq $elem) { throw "AutomationElement.FromHandle returned null for $Hwnd" }
    $br = $elem.Current.BoundingRectangle
    $ox = if ([double]::IsInfinity($br.X) -or [double]::IsNaN($br.X)) { 0.0 } else { $br.X }
    $oy = if ([double]::IsInfinity($br.Y) -or [double]::IsNaN($br.Y)) { 0.0 } else { $br.Y }
    $result = Dump-UiaElement -Elem $elem -Depth 0 -LimitDepth $MaxDepth -OriginX $ox -OriginY $oy
}
elseif ($Mode -eq "win32") {
    if ($Hwnd -eq 0) {
        $wins = Get-WindowInfoList -ProcessId ([uint32]$proc.Id) -IncludeHiddenFlag $false -DesktopName $Desktop
        $mainWin = $wins | Where-Object { $_.menuCount -gt 0 } | Select-Object -First 1
        if ($mainWin) { $Hwnd = $mainWin.hwnd } else { $Hwnd = $proc.MainWindowHandle.ToInt64() }
    }
    $result = [ordered]@{
        hwnd = $Hwnd
        desktop = $Desktop
        children = @(Dump-Win32Children -ParentHwnd ([IntPtr]$Hwnd) -DesktopName $Desktop)
    }
}
else {
    throw "Unknown mode: $Mode"
}

$json = $result | ConvertTo-Json -Depth 30
if ($OutPath) {
    $dir = Split-Path -Parent $OutPath
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    [System.IO.File]::WriteAllText($OutPath, $json, [System.Text.UTF8Encoding]::new($false))
    Write-Output "Wrote $OutPath"
} else {
    Write-Output $json
}
