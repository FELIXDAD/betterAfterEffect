<#
.SYNOPSIS
    Extract the menu tree from a running Adobe Illustrator instance on Windows without clicking or activating the window.
.DESCRIPTION
    First attempts read-only Win32 user32 menu enumeration (GetMenu / GetMenuItemCount / GetMenuStringW / GetSubMenu / GetMenuItemInfoW).
    Falls back to read-only UI Automation tree traversal if the Win32 menu handle is unavailable or empty.
#>
param(
    [string]$OutputPath = "E:\bae-test\ref-ai\menus_raw.json",
    [string]$InstallDir = "C:\Program Files\Adobe\Adobe Illustrator 2026"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class Win32MenuReader
{
    public const uint MF_BYPOSITION = 0x00000400;
    public const uint MIIM_STATE    = 0x00000001;
    public const uint MIIM_ID       = 0x00000002;
    public const uint MIIM_SUBMENU  = 0x00000004;
    public const uint MIIM_FTYPE    = 0x00000100;
    public const uint MFT_SEPARATOR = 0x00000800;
    public const uint MFS_DISABLED  = 0x00000003;

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

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowTextW(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassNameW(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern IntPtr GetMenu(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern int GetMenuItemCount(IntPtr hMenu);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetMenuStringW(IntPtr hMenu, uint uIDItem, StringBuilder lpString, int nMaxCount, uint uFlag);

    [DllImport("user32.dll")]
    public static extern IntPtr GetSubMenu(IntPtr hMenu, int nPos);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool GetMenuItemInfoW(IntPtr hMenu, uint uItem, bool fByPosition, ref MENUITEMINFO lpmii);

    [DllImport("user32.dll")]
    public static extern bool EnumChildWindows(IntPtr hWndParent, EnumWindowsProc lpEnumFunc, IntPtr lParam);

    public static List<IntPtr> FindChildWindows(IntPtr parent)
    {
        var result = new List<IntPtr>();
        EnumChildWindows(parent, (hWnd, lParam) => {
            result.Add(hWnd);
            return true;
        }, IntPtr.Zero);
        return result;
    }

    public static List<IntPtr> FindProcessWindows(uint pid)
    {
        var result = new List<IntPtr>();
        EnumWindows((hWnd, lParam) => {
            uint winPid;
            GetWindowThreadProcessId(hWnd, out winPid);
            if (winPid == pid)
            {
                result.Add(hWnd);
            }
            return true;
        }, IntPtr.Zero);
        return result;
    }

    public static string GetWindowTitle(IntPtr hWnd)
    {
        var sb = new StringBuilder(512);
        GetWindowTextW(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }

    public static string GetWindowClass(IntPtr hWnd)
    {
        var sb = new StringBuilder(256);
        GetClassNameW(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }
}
"@

function Clean-Mnemonic {
    param([string]$Text)
    if ([string]::IsNullOrEmpty($Text)) { return "" }
    # Convert escaped "&&" to placeholder, strip single "&" mnemonic markers, restore "&".
    $placeholder = [char]0xE000
    $s = $Text.Replace("&&", [string]$placeholder)
    $s = $s.Replace("&", "")
    $s = $s.Replace([string]$placeholder, "&")
    return $s.Trim()
}

function Parse-MenuText {
    param([string]$RawText)
    $parts = $RawText -split "[`t`b]", 2
    $label = Clean-Mnemonic $parts[0]
    $shortcut = if ($parts.Length -gt 1) { $parts[1].Trim() } else { $null }
    return @{
        label = $label
        shortcut = $shortcut
        raw = $RawText
    }
}

function Read-Win32MenuRecursive {
    param(
        [IntPtr]$HMenu,
        [int]$Depth = 0
    )
    if ($HMenu -eq [IntPtr]::Zero -or $Depth -gt 12) { return @() }
    $count = [Win32MenuReader]::GetMenuItemCount($HMenu)
    if ($count -le 0) { return @() }

    $items = New-Object System.Collections.ArrayList
    for ($i = 0; $i -lt $count; $i++) {
        $mii = New-Object Win32MenuReader+MENUITEMINFO
        $mii.cbSize = [uint32][System.Runtime.InteropServices.Marshal]::SizeOf($mii)
        $mii.fMask = [Win32MenuReader]::MIIM_FTYPE -bor [Win32MenuReader]::MIIM_STATE -bor [Win32MenuReader]::MIIM_ID -bor [Win32MenuReader]::MIIM_SUBMENU
        $ok = [Win32MenuReader]::GetMenuItemInfoW($HMenu, [uint32]$i, $true, [ref]$mii)

        $isSeparator = $ok -and (($mii.fType -band [Win32MenuReader]::MFT_SEPARATOR) -ne 0)
        $sb = New-Object System.Text.StringBuilder 1024
        $len = [Win32MenuReader]::GetMenuStringW($HMenu, [uint32]$i, $sb, $sb.Capacity, [Win32MenuReader]::MF_BYPOSITION)
        $rawText = if ($len -gt 0) { $sb.ToString() } else { "" }

        if ($isSeparator -and [string]::IsNullOrWhiteSpace($rawText)) {
            [void]$items.Add([ordered]@{
                type = "separator"
            })
            continue
        }

        $parsed = Parse-MenuText $rawText
        $subMenu = [Win32MenuReader]::GetSubMenu($HMenu, $i)
        $hasSub = ($subMenu -ne [IntPtr]::Zero)
        $children = if ($hasSub) { @(Read-Win32MenuRecursive -HMenu $subMenu -Depth ($Depth + 1)) } else { @() }

        $entry = [ordered]@{
            type = if ($hasSub) { "submenu" } else { "item" }
            label = $parsed.label
        }
        if ($parsed.shortcut) {
            $entry["shortcut"] = $parsed.shortcut
        }
        if (-not $hasSub -and $ok -and $mii.wID -ne 0) {
            $entry["commandId"] = [int]$mii.wID
        }
        if ($hasSub) {
            $entry["children"] = $children
        }
        [void]$items.Add($entry)
    }
    return $items.ToArray()
}

function Read-UiaMenuTree {
    param([IntPtr]$HWnd)
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes

    $root = [System.Windows.Automation.AutomationElement]::FromHandle($HWnd)
    if ($null -eq $root) { return @() }

    $menuBarCond = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::MenuBar
    )
    $menuBars = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $menuBarCond)
    if ($menuBars.Count -eq 0) { return @() }

    function Walk-UiaNode {
        param($El, [int]$Depth = 0)
        if ($Depth -gt 12) { return @() }
        $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
        $child = $walker.GetFirstChild($El)
        $list = New-Object System.Collections.ArrayList
        while ($null -ne $child) {
            $ct = $child.Current.ControlType
            $name = $child.Current.Name
            $accel = $child.Current.AcceleratorKey
            $sub = @(Walk-UiaNode -El $child -Depth ($Depth + 1))
            if ($ct -eq [System.Windows.Automation.ControlType]::Separator) {
                [void]$list.Add([ordered]@{ type = "separator" })
            } elseif (-not [string]::IsNullOrWhiteSpace($name) -or $sub.Count -gt 0) {
                $node = [ordered]@{
                    type = if ($sub.Count -gt 0) { "submenu" } else { "item" }
                    label = $name
                }
                if (-not [string]::IsNullOrWhiteSpace($accel)) {
                    $node["shortcut"] = $accel
                }
                if ($sub.Count -gt 0) {
                    $node["children"] = $sub
                }
                [void]$list.Add($node)
            }
            $child = $walker.GetNextSibling($child)
        }
        return $list.ToArray()
    }

    $allMenus = New-Object System.Collections.ArrayList
    foreach ($mb in $menuBars) {
        $mbName = $mb.Current.Name
        # Skip system titlebar menu bar if multiple exist and one is application menu bar
        $children = @(Walk-UiaNode -El $mb -Depth 0)
        [void]$allMenus.Add([ordered]@{
            menuBarName = $mbName
            items = $children
        })
    }
    return $allMenus.ToArray()
}

$proc = Get-Process -Name "Illustrator" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $proc) {
    throw "Illustrator process is not running."
}

$windows = [Win32MenuReader]::FindProcessWindows([uint32]$proc.Id)
$winSummary = @()
$childSummary = @()
$targetHwnd = [IntPtr]::Zero
$targetHmenu = [IntPtr]::Zero
$illustratorFrameHwnd = [IntPtr]::Zero
$win32Menus = @()

foreach ($w in $windows) {
    $hm = [Win32MenuReader]::GetMenu($w)
    $cnt = if ($hm -ne [IntPtr]::Zero) { [Win32MenuReader]::GetMenuItemCount($hm) } else { 0 }
    $title = [Win32MenuReader]::GetWindowTitle($w)
    $cls = [Win32MenuReader]::GetWindowClass($w)
    $vis = [Win32MenuReader]::IsWindowVisible($w)
    $winSummary += [ordered]@{
        hwnd = $w.ToInt64()
        className = $cls
        title = $title
        visible = $vis
        hasHMenu = ($hm -ne [IntPtr]::Zero)
        topMenuCount = $cnt
    }
    if ($cls -eq "illustrator" -and $vis -and $illustratorFrameHwnd -eq [IntPtr]::Zero) {
        $illustratorFrameHwnd = $w
    }
    if ($cnt -gt 0 -and $targetHmenu -eq [IntPtr]::Zero) {
        $targetHwnd = $w
        $targetHmenu = $hm
    }
}

# Also inspect child windows of the Illustrator frame window for Win32 HMENU (e.g. OWL menu bar host)
if ($illustratorFrameHwnd -ne [IntPtr]::Zero) {
    $children = [Win32MenuReader]::FindChildWindows($illustratorFrameHwnd)
    foreach ($cw in $children) {
        $chm = [Win32MenuReader]::GetMenu($cw)
        $ccnt = if ($chm -ne [IntPtr]::Zero) { [Win32MenuReader]::GetMenuItemCount($chm) } else { 0 }
        $ctitle = [Win32MenuReader]::GetWindowTitle($cw)
        $ccls = [Win32MenuReader]::GetWindowClass($cw)
        $cvis = [Win32MenuReader]::IsWindowVisible($cw)
        $childSummary += [ordered]@{
            hwnd = $cw.ToInt64()
            className = $ccls
            title = $ctitle
            visible = $cvis
            hasHMenu = ($chm -ne [IntPtr]::Zero)
            topMenuCount = $ccnt
        }
        if ($ccnt -gt 0 -and $targetHmenu -eq [IntPtr]::Zero) {
            $targetHwnd = $cw
            $targetHmenu = $chm
        }
    }
}

$method = "none"
$uiaFallback = @()
if ($targetHmenu -ne [IntPtr]::Zero) {
    $win32Menus = @(Read-Win32MenuRecursive -HMenu $targetHmenu -Depth 0)
    $method = "win32_user32_GetMenu"
} else {
    $mainHwnd = if ($illustratorFrameHwnd -ne [IntPtr]::Zero) {
        $illustratorFrameHwnd
    } elseif ($proc.MainWindowHandle -ne [IntPtr]::Zero) {
        $proc.MainWindowHandle
    } elseif ($windows.Count -gt 0) {
        $windows[0]
    } else {
        [IntPtr]::Zero
    }
    if ($mainHwnd -ne [IntPtr]::Zero) {
        $uiaFallback = @(Read-UiaMenuTree -HWnd $mainHwnd)
        $method = "uia_readonly"
    }
}

$osInfo = Get-CimInstance Win32_OperatingSystem
$exePath = Join-Path $InstallDir "Support Files\Contents\Windows\Illustrator.exe"
$exeVer = if (Test-Path $exePath) { (Get-Item $exePath).VersionInfo } else { $null }

$payload = [ordered]@{
    method = $method
    extractedAt = (Get-Date).ToString("o")
    illustratorFrameHwnd = $illustratorFrameHwnd.ToInt64()
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
    processWindows = $winSummary
    frameChildWindows = $childSummary
    menus = $win32Menus
    uiaMenuBars = $uiaFallback
}

$outDir = Split-Path -Parent $OutputPath
if ($outDir -and -not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Force $outDir | Out-Null
}
$json = $payload | ConvertTo-Json -Depth 30
[System.IO.File]::WriteAllText($OutputPath, $json, [System.Text.UTF8Encoding]::new($false))
Write-Host "Wrote $OutputPath using method=$method topMenus=$($win32Menus.Count) uiaBars=$($uiaFallback.Count) frameChildren=$($childSummary.Count)"
