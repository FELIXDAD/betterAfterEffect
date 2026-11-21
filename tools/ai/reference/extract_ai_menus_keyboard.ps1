<#
.SYNOPSIS
    Extract the complete menu tree from a running Adobe Illustrator instance on Windows using keyboard-only menu opening.
.DESCRIPTION
    Illustrator 2026 uses Adobe OWL's owner-drawn OWL.MenuBar, which does not expose a static Win32 HMENU or UIA children
    while closed. This script focuses the Illustrator frame on WinSta0\Default, sends keyboard-only Alt+<mnemonic> hotkeys
    via `orca computer hotkey` (never clicking or executing any menu command), reads each opened Win32 #32768 popup menu
    recursively via MN_GETHMENU + user32 APIs, and immediately closes each menu with Escape.
#>
param(
    [string]$OutputPath = "E:\bae-test\ref-ai\menus_raw.json",
    [string]$InstallDir = "C:\Program Files\Adobe\Adobe Illustrator 2026",
    [int]$DelayMs = 350
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public class MenuNodeDto
{
    public string type;
    public string label;
    public string shortcut;
    public int commandId;
    public List<MenuNodeDto> children;
}

public class PopupReadDto
{
    public int popupCount;
    public long popupHwnd;
    public long hmenu;
    public int emptySubmenus;
    public List<string> emptySubmenuNames = new List<string>();
    public List<MenuNodeDto> items = new List<MenuNodeDto>();
    public List<string> visibleWindows = new List<string>();
}

public static class DefaultDesktopMenuScanner
{
    public const uint MF_BYPOSITION = 0x00000400;
    public const uint MIIM_STATE    = 0x00000001;
    public const uint MIIM_ID       = 0x00000002;
    public const uint MIIM_SUBMENU  = 0x00000004;
    public const uint MIIM_FTYPE    = 0x00000100;
    public const uint MFT_SEPARATOR = 0x00000800;
    public const uint MN_GETHMENU   = 0x01E1;

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct MENUITEMINFO
    {
        public uint cbSize;
        public uint fMask;
        public uint fType;
        public uint fState;
        public uint wID;
        public IntPtr hSubMenu;
        public IntPtr hbmpChecked;
        public IntPtr hbmpUnchecked;
        public IntPtr dwItemData;
        public string dwTypeData;
        public uint cch;
        public IntPtr hbmpItem;
    }

    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern IntPtr OpenDesktopW(string lpszDesktop, uint dwFlags, bool fInherit, uint dwDesiredAccess);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool SetThreadDesktop(IntPtr hDesktop);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool CloseDesktop(IntPtr hDesktop);

    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr hMenu);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetMenuStringW(IntPtr hMenu, uint uIDItem, StringBuilder lpString, int nMaxCount, uint uFlag);
    [DllImport("user32.dll")] public static extern IntPtr GetSubMenu(IntPtr hMenu, int nPos);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool GetMenuItemInfoW(IntPtr hMenu, uint uItem, bool fByPosition, ref MENUITEMINFO lpmii);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    private static string GetClass(IntPtr hWnd)
    {
        var sb = new StringBuilder(256);
        GetClassNameW(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }

    private static string GetTitle(IntPtr hWnd)
    {
        var sb = new StringBuilder(512);
        GetWindowTextW(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }

    private static string CleanMnemonic(string text)
    {
        if (string.IsNullOrEmpty(text)) return "";
        char ph = '\uE000';
        return text.Replace("&&", ph.ToString()).Replace("&", "").Replace(ph.ToString(), "&").Trim();
    }

    public static bool FocusIllustratorFrame(uint pid)
    {
        bool ok = false;
        var worker = new Thread(() => {
            IntPtr hDesk = OpenDesktopW("Default", 0, false, 0x0001 | 0x0002 | 0x0040 | 0x0080 | 0x0100);
            if (hDesk == IntPtr.Zero) return;
            try
            {
                if (!SetThreadDesktop(hDesk)) return;
                IntPtr frame = IntPtr.Zero;
                EnumWindows((hWnd, lParam) => {
                    uint p;
                    GetWindowThreadProcessId(hWnd, out p);
                    if (p == pid && IsWindowVisible(hWnd) &&
                        string.Equals(GetClass(hWnd), "illustrator", StringComparison.OrdinalIgnoreCase))
                    {
                        frame = hWnd;
                        return false;
                    }
                    return true;
                }, IntPtr.Zero);
                if (frame == IntPtr.Zero) return;

                IntPtr fg = GetForegroundWindow();
                uint fgPid;
                uint fgThread = (fg != IntPtr.Zero) ? GetWindowThreadProcessId(fg, out fgPid) : 0;
                uint targetPid;
                uint targetThread = GetWindowThreadProcessId(frame, out targetPid);
                uint curThread = GetCurrentThreadId();
                bool attachedFg = false;
                bool attachedTarget = false;
                try
                {
                    if (fgThread != 0 && fgThread != curThread)
                        attachedFg = AttachThreadInput(curThread, fgThread, true);
                    if (targetThread != 0 && targetThread != curThread)
                        attachedTarget = AttachThreadInput(curThread, targetThread, true);
                    if (IsIconic(frame)) ShowWindow(frame, 9);
                    BringWindowToTop(frame);
                    SetForegroundWindow(frame);
                }
                finally
                {
                    if (attachedTarget) AttachThreadInput(curThread, targetThread, false);
                    if (attachedFg) AttachThreadInput(curThread, fgThread, false);
                }
                Thread.Sleep(150);
                ok = (GetForegroundWindow() == frame);
            }
            finally
            {
                CloseDesktop(hDesk);
            }
        });
        worker.SetApartmentState(ApartmentState.STA);
        worker.Start();
        worker.Join();
        return ok;
    }

    private static List<MenuNodeDto> ReadHMenuRecursive(IntPtr hMenu, int depth, ref int emptySubmenus, List<string> emptyNames)
    {
        var items = new List<MenuNodeDto>();
        if (hMenu == IntPtr.Zero || depth > 12) return items;
        int count = GetMenuItemCount(hMenu);
        if (count <= 0) return items;

        for (int i = 0; i < count; i++)
        {
            var mii = new MENUITEMINFO();
            mii.cbSize = (uint)Marshal.SizeOf(typeof(MENUITEMINFO));
            mii.fMask = MIIM_FTYPE | MIIM_STATE | MIIM_ID | MIIM_SUBMENU;
            bool ok = GetMenuItemInfoW(hMenu, (uint)i, true, ref mii);

            bool isSep = ok && ((mii.fType & MFT_SEPARATOR) != 0);
            var sb = new StringBuilder(1024);
            int len = GetMenuStringW(hMenu, (uint)i, sb, sb.Capacity, MF_BYPOSITION);
            string raw = len > 0 ? sb.ToString() : "";

            if (isSep && string.IsNullOrWhiteSpace(raw))
            {
                items.Add(new MenuNodeDto { type = "separator" });
                continue;
            }

            string label = raw;
            string shortcut = null;
            int sepIdx = raw.IndexOfAny(new char[] { '\t', '\b' });
            if (sepIdx >= 0)
            {
                label = raw.Substring(0, sepIdx);
                shortcut = raw.Substring(sepIdx + 1).Trim();
            }
            label = CleanMnemonic(label);

            IntPtr sub = GetSubMenu(hMenu, i);
            bool hasSub = sub != IntPtr.Zero;
            var node = new MenuNodeDto
            {
                type = hasSub ? "submenu" : "item",
                label = label,
                shortcut = string.IsNullOrEmpty(shortcut) ? null : shortcut,
                commandId = (!hasSub && ok) ? (int)mii.wID : 0
            };
            if (hasSub)
            {
                node.children = ReadHMenuRecursive(sub, depth + 1, ref emptySubmenus, emptyNames);
                if (node.children.Count == 0)
                {
                    emptySubmenus++;
                    emptyNames.Add(label);
                }
            }
            items.Add(node);
        }
        return items;
    }

    public static PopupReadDto ReadCurrentPopup(uint pid)
    {
        var dto = new PopupReadDto();
        var worker = new Thread(() => {
            IntPtr hDesk = OpenDesktopW("Default", 0, false, 0x0001 | 0x0040 | 0x0080 | 0x0100);
            if (hDesk == IntPtr.Zero) return;
            try
            {
                if (!SetThreadDesktop(hDesk)) return;
                var popups = new List<IntPtr>();
                EnumWindows((hWnd, lParam) => {
                    if (!IsWindowVisible(hWnd)) return true;
                    uint p;
                    GetWindowThreadProcessId(hWnd, out p);
                    string cls = GetClass(hWnd);
                    if (cls == "#32768" && p == pid)
                    {
                        popups.Add(hWnd);
                    }
                    if (p == pid || cls == "#32768")
                    {
                        dto.visibleWindows.Add(string.Format("{0}|pid={1}|{2}|{3}", hWnd.ToInt64(), p, cls, GetTitle(hWnd)));
                    }
                    return true;
                }, IntPtr.Zero);

                dto.popupCount = popups.Count;
                if (popups.Count > 0)
                {
                    dto.popupHwnd = popups[0].ToInt64();
                    IntPtr hMenu = SendMessage(popups[0], MN_GETHMENU, IntPtr.Zero, IntPtr.Zero);
                    dto.hmenu = hMenu.ToInt64();
                    if (hMenu != IntPtr.Zero)
                    {
                        int emptySubs = 0;
                        dto.items = ReadHMenuRecursive(hMenu, 0, ref emptySubs, dto.emptySubmenuNames);
                        dto.emptySubmenus = emptySubs;
                    }
                }
            }
            finally
            {
                CloseDesktop(hDesk);
            }
        });
        worker.SetApartmentState(ApartmentState.STA);
        worker.Start();
        worker.Join();
        return dto;
    }
}
"@

function Convert-NodeDto {
    param($Node)
    if ($Node.type -eq "separator") {
        return [ordered]@{ type = "separator" }
    }
    $out = [ordered]@{
        type = $Node.type
        label = $Node.label
    }
    if ($Node.shortcut) {
        $out["shortcut"] = $Node.shortcut
    }
    if ($Node.type -eq "item" -and $Node.commandId -ne 0) {
        $out["commandId"] = $Node.commandId
    }
    if ($Node.type -eq "submenu") {
        $ch = @()
        if ($Node.children) {
            foreach ($c in $Node.children) {
                $ch += Convert-NodeDto $c
            }
        }
        $out["children"] = $ch
    }
    return $out
}

function Close-PopupViaOrca {
    param([uint32]$ProcId)
    for ($i = 0; $i -lt 3; $i++) {
        orca computer press-key --app Illustrator --key Escape --no-screenshot --json | Out-Null
        Start-Sleep -Milliseconds 120
        $chk = [DefaultDesktopMenuScanner]::ReadCurrentPopup($ProcId)
        if ($chk.popupCount -eq 0) { break }
    }
}

$proc = Get-Process -Name "Illustrator" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $proc) {
    throw "Illustrator process is not running."
}
$procId = [uint32]$proc.Id

$topMenuSpecs = @(
    @{ key = "File";   label = ([char]0x6587 + [char]0x4EF6); hotkey = "Alt+f" }, # 文件
    @{ key = "Edit";   label = ([char]0x7F16 + [char]0x8F91); hotkey = "Alt+e" }, # 编辑
    @{ key = "Object"; label = ([char]0x5BF9 + [char]0x8C61); hotkey = "Alt+o" }, # 对象
    @{ key = "Type";   label = ([char]0x6587 + [char]0x5B57); hotkey = "Alt+t" }, # 文字
    @{ key = "Select"; label = ([char]0x9009 + [char]0x62E9); hotkey = "Alt+s" }, # 选择
    @{ key = "Effect"; label = ([char]0x6548 + [char]0x679C); hotkey = "Alt+c" }, # 效果
    @{ key = "View";   label = ([char]0x89C6 + [char]0x56FE); hotkey = "Alt+v" }, # 视图
    @{ key = "Window"; label = ([char]0x7A97 + [char]0x53E3); hotkey = "Alt+w" }, # 窗口
    @{ key = "Help";   label = ([char]0x5E2E + [char]0x52A9); hotkey = "Alt+h" }  # 帮助
)

$extractedMenus = New-Object System.Collections.ArrayList
$diagnostics = New-Object System.Collections.ArrayList

try {
    foreach ($spec in $topMenuSpecs) {
        $fgOk = $false
        $scan = $null
        $hkStatus = ""
        for ($attempt = 0; $attempt -lt 2; $attempt++) {
            $fgOk = [DefaultDesktopMenuScanner]::FocusIllustratorFrame($procId)
            Start-Sleep -Milliseconds 150
            $hkOut = (orca computer hotkey --app Illustrator --key $spec.hotkey --no-screenshot --json 2>&1) -join " "
            $hkStatus = if ($hkOut -match '"ok":\s*true') { "ok" } else { $hkOut.Substring(0, [Math]::Min(160, $hkOut.Length)) }
            Start-Sleep -Milliseconds $DelayMs

            $scan = [DefaultDesktopMenuScanner]::ReadCurrentPopup($procId)
            if ($scan.popupCount -gt 0) { break }
            Start-Sleep -Milliseconds 400
        }

        $children = @()
        if ($scan.items) {
            foreach ($it in $scan.items) {
                $children += Convert-NodeDto $it
            }
        }

        # Immediately close menu with Escape
        Close-PopupViaOrca -ProcId $procId
        Start-Sleep -Milliseconds 200

        $diag = [ordered]@{
            key = $spec.key
            menu = $spec.label
            hotkey = $spec.hotkey
            fgOk = $fgOk
            hkStatus = $hkStatus
            popupCount = $scan.popupCount
            hmenu = $scan.hmenu
            itemCount = $children.Count
            emptySubmenus = $scan.emptySubmenus
            emptySubmenuNames = @($scan.emptySubmenuNames)
        }
        if ($scan.popupCount -eq 0) {
            $diag["visibleWindows"] = @($scan.visibleWindows)
        }
        [void]$diagnostics.Add($diag)
        [void]$extractedMenus.Add([ordered]@{
            type = "submenu"
            label = $spec.label
            children = $children
        })
        Write-Host ("Menu '{0}' ({1}): fg={2} hk={3} popups={4} items={5} emptySubs={6}" -f $spec.key, $spec.hotkey, $fgOk, $hkStatus, $scan.popupCount, $children.Count, $scan.emptySubmenus)
    }
} finally {
    Close-PopupViaOrca -ProcId $procId
}

$finalCheck = [DefaultDesktopMenuScanner]::ReadCurrentPopup($procId)
$osInfo = Get-CimInstance Win32_OperatingSystem
$exePath = Join-Path $InstallDir "Support Files\Contents\Windows\Illustrator.exe"
$exeVer = if (Test-Path $exePath) { (Get-Item $exePath).VersionInfo } else { $null }

$payload = [ordered]@{
    method = "win32_orca_hotkey_alt_MN_GETHMENU"
    extractedAt = (Get-Date).ToString("o")
    remainingOpenPopups = $finalCheck.popupCount
    windows = [ordered]@{
        caption = $osInfo.Caption
        version = $osInfo.Version
        buildNumber = $osInfo.BuildNumber
        architecture = $osInfo.OSArchitecture
    }
    executable = if ($exeVer) {
        [ordered]@{
            productName = $exeVer.ProductName
            productVersion = $exeVer.ProductVersion
            fileVersion = $exeVer.FileVersion
        }
    } else { $null }
    diagnostics = $diagnostics.ToArray()
    menus = $extractedMenus.ToArray()
}

$outDir = Split-Path -Parent $OutputPath
if ($outDir -and -not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Force $outDir | Out-Null
}
$json = $payload | ConvertTo-Json -Depth 30
[System.IO.File]::WriteAllText($OutputPath, $json, [System.Text.UTF8Encoding]::new($false))
Write-Host "Wrote $OutputPath (remainingOpenPopups=$($finalCheck.popupCount))"
