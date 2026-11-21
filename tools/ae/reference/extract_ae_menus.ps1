param(
    [string]$OutPath = "E:\bae-test\ref-ae\raw\menus_raw.json"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class AeMenuReader {
    public const uint MF_BYPOSITION = 0x00000400;
    public const uint MF_SEPARATOR  = 0x00000800;
    public const uint MF_GRAYED     = 0x00000001;
    public const uint MF_DISABLED   = 0x00000002;
    public const uint MF_CHECKED    = 0x00000008;
    public const uint MF_POPUP      = 0x00000010;

    [DllImport("user32.dll")]
    public static extern IntPtr GetMenu(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern int GetMenuItemCount(IntPtr hMenu);

    [DllImport("user32.dll")]
    public static extern IntPtr GetSubMenu(IntPtr hMenu, int nPos);

    [DllImport("user32.dll")]
    public static extern uint GetMenuItemID(IntPtr hMenu, int nPos);

    [DllImport("user32.dll")]
    public static extern uint GetMenuState(IntPtr hMenu, uint uId, uint uFlags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetMenuString(IntPtr hMenu, uint uIDItem, StringBuilder lpString, int nMaxCount, uint uFlag);

    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    public static List<IntPtr> FindProcessWindowsWithMenu(uint pid) {
        var result = new List<IntPtr>();
        EnumWindows((hWnd, lParam) => {
            uint wPid;
            GetWindowThreadProcessId(hWnd, out wPid);
            if (wPid == pid) {
                IntPtr hMenu = GetMenu(hWnd);
                if (hMenu != IntPtr.Zero && GetMenuItemCount(hMenu) > 0) {
                    result.Add(hWnd);
                }
            }
            return true;
        }, IntPtr.Zero);
        return result;
    }

    public static string ReadMenuString(IntPtr hMenu, uint pos) {
        var sb = new StringBuilder(512);
        int len = GetMenuString(hMenu, pos, sb, sb.Capacity, MF_BYPOSITION);
        if (len <= 0) return "";
        return sb.ToString();
    }

    public static string ReadWindowTitle(IntPtr hWnd) {
        var sb = new StringBuilder(512);
        GetWindowText(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }

    public static string ReadClassName(IntPtr hWnd) {
        var sb = new StringBuilder(256);
        GetClassName(hWnd, sb, sb.Capacity);
        return sb.ToString();
    }
}
"@

function Read-Win32MenuRecursive {
    param(
        [IntPtr]$HMenu,
        [int]$Depth = 0
    )
    $items = New-Object System.Collections.ArrayList
    if ($HMenu -eq [IntPtr]::Zero -or $Depth -gt 12) {
        return ,$items
    }
    $count = [AeMenuReader]::GetMenuItemCount($HMenu)
    for ($i = 0; $i -lt $count; $i++) {
        $state = [AeMenuReader]::GetMenuState($HMenu, [uint32]$i, [AeMenuReader]::MF_BYPOSITION)
        $isSep = (($state -band [AeMenuReader]::MF_SEPARATOR) -ne 0)
        $rawText = [AeMenuReader]::ReadMenuString($HMenu, [uint32]$i)
        $sub = [AeMenuReader]::GetSubMenu($HMenu, $i)
        $cmdId = [AeMenuReader]::GetMenuItemID($HMenu, $i)

        if ($isSep -and [string]::IsNullOrEmpty($rawText) -and $sub -eq [IntPtr]::Zero) {
            [void]$items.Add([ordered]@{
                type = "separator"
            })
            continue
        }

        $label = $rawText
        $shortcut = $null
        if ($rawText -match "^(.*?)\t(.*)$") {
            $label = $Matches[1]
            $shortcut = $Matches[2]
        }
        # Strip Win32 accelerator ampersand markers while keeping literal &&
        $cleanLabel = $label -replace "&&", "__AMP__" -replace "&", "" -replace "__AMP__", "&"

        $node = [ordered]@{
            type = if ($sub -ne [IntPtr]::Zero) { "submenu" } else { "item" }
            label = $cleanLabel
        }
        if ($shortcut) {
            $node["shortcut"] = $shortcut
        }
        if ($sub -eq [IntPtr]::Zero -and $cmdId -ne [uint32]::MaxValue) {
            $node["commandId"] = [int64]$cmdId
        }
        if ($sub -ne [IntPtr]::Zero) {
            $children = Read-Win32MenuRecursive -HMenu $sub -Depth ($Depth + 1)
            $node["items"] = @($children)
        }
        [void]$items.Add($node)
    }
    return ,$items
}

function Read-UiaMenuReadOnly {
    param([IntPtr]$HWnd)
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($HWnd)
    if ($null -eq $root) { return @() }
    $cond = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::MenuBar
    )
    $bars = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
    $result = New-Object System.Collections.ArrayList
    foreach ($bar in $bars) {
        $barItems = New-Object System.Collections.ArrayList
        $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
        $child = $walker.GetFirstChild($bar)
        while ($null -ne $child) {
            $entry = [ordered]@{
                label = $child.Current.Name
                automationId = $child.Current.AutomationId
                controlType = $child.Current.ControlType.ProgrammaticName
                items = @()
            }
            # Read-only traversal of any already-materialized children
            $subItems = New-Object System.Collections.ArrayList
            $subChild = $walker.GetFirstChild($child)
            while ($null -ne $subChild) {
                [void]$subItems.Add([ordered]@{
                    label = $subChild.Current.Name
                    controlType = $subChild.Current.ControlType.ProgrammaticName
                })
                $subChild = $walker.GetNextSibling($subChild)
            }
            $entry["items"] = @($subItems)
            [void]$barItems.Add($entry)
            $child = $walker.GetNextSibling($child)
        }
        [void]$result.Add([ordered]@{
            menuBarName = $bar.Current.Name
            items = @($barItems)
        })
    }
    return ,@($result)
}

$aeProc = Get-Process -Name "AfterFX" -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $aeProc) {
    throw "AfterFX process is not running."
}

$win32Windows = [AeMenuReader]::FindProcessWindowsWithMenu([uint32]$aeProc.Id)
$output = [ordered]@{
    pid = $aeProc.Id
    method = "none"
    windowClass = ""
    menus = @()
    uiaFallback = $null
}

if ($win32Windows.Count -gt 0) {
    $hwnd = $win32Windows[0]
    $hMenu = [AeMenuReader]::GetMenu($hwnd)
    $tree = Read-Win32MenuRecursive -HMenu $hMenu -Depth 0
    $output["method"] = "win32_user32_GetMenu"
    $output["windowClass"] = [AeMenuReader]::ReadClassName($hwnd)
    $output["menus"] = @($tree)
} else {
    $hwnd = $aeProc.MainWindowHandle
    $output["method"] = "uia_readonly"
    $output["windowClass"] = if ($hwnd -ne [IntPtr]::Zero) { [AeMenuReader]::ReadClassName($hwnd) } else { "" }
    if ($hwnd -ne [IntPtr]::Zero) {
        try {
            $output["uiaFallback"] = Read-UiaMenuReadOnly -HWnd $hwnd
        } catch {
            $output["uiaError"] = $_.Exception.Message
        }
    }
}

$dir = Split-Path -Parent $OutPath
if ($dir -and -not (Test-Path $dir)) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}
$json = $output | ConvertTo-Json -Depth 25 -Compress
[System.IO.File]::WriteAllText($OutPath, $json, [System.Text.UTF8Encoding]::new($false))
Write-Output ("Method: " + $output["method"] + ", top-level menus: " + $output["menus"].Count)
