# AE shortcut sweep on a real Windows PC: presses every AE default shortcut BAE
# has implemented as a real key event (SendInput) and checks which action BAE ran.
#
# Rows come from the AE command catalog: the script generates the key table when
# it starts (BAE_SHORTCUTS_JSON=<file> cargo test -p bae-ui-motion shortcuts_json),
# or reads -Table. BAE runs with BAE_HOME in a temporary folder (the user's settings and
# projects are not touched) and BAE_ACTION_LOG=<file>, which makes it append
# every action it dispatches as a JSON line; after each key the script reads the
# new lines. SendInput goes to the foreground window: the script brings BAE to
# the front and needs the desktop for the run.
#
#   cargo build --release -p bae
#   powershell -ExecutionPolicy Bypass -File tools\qa\shortcut_sweep_windows.ps1 `
#       -Bae target\release\bae.exe -Report docs\test-reports\win5090\<date>-shortcut-sweep.md

param(
    [string]$Bae = "target\release\bae.exe",
    [string]$Table = "",
    [string]$Report = "",
    [double]$Settle = 0.6,
    [int]$Limit = 0
)

$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Keys {
    [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Explicit)] struct INPUTUNION { [FieldOffset(0)] public KEYBDINPUT ki; [FieldOffset(0)] public long pad0; [FieldOffset(8)] public long pad1; [FieldOffset(16)] public long pad2; }
    [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public INPUTUNION u; }
    [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    const uint KEYUP = 2;
    static INPUT K(ushort vk, bool up) { var i = new INPUT(); i.type = 1; i.u.ki.wVk = vk; i.u.ki.dwFlags = up ? KEYUP : 0; return i; }
    public static void Chord(ushort[] mods, ushort key) {
        var list = new System.Collections.Generic.List<INPUT>();
        foreach (var m in mods) list.Add(K(m, false));
        list.Add(K(key, false)); list.Add(K(key, true));
        for (int j = mods.Length - 1; j >= 0; j--) list.Add(K(mods[j], true));
        SendInput((uint)list.Count, list.ToArray(), Marshal.SizeOf(typeof(INPUT)));
    }
}
"@

# Actions not pressed: they quit, close the project or panels, or open file /
# save dialogs or a web browser.
$Skip = @{
    "app.quit" = "quits"; "window.close_panel" = "closes the focused panel";
    "file.new_project" = "replaces the test project"; "file.close_project" = "closes the test project";
    "file.open_project" = "opens a file dialog"; "file.save" = "opens a save dialog";
    "file.save_as" = "opens a save dialog"; "help.docs" = "opens a web browser";
    "motion.file.import_file" = "opens a file dialog"; "motion.file.import_multiple_files" = "opens a file dialog"
}

# BAE key names (egui) -> virtual-key codes.
$Vk = @{
    "Equals" = 0xBB; "Minus" = 0xBD; "OpenBracket" = 0xDB; "CloseBracket" = 0xDD; "Period" = 0xBE;
    "Comma" = 0xBC; "Slash" = 0xBF; "Backslash" = 0xDC; "Semicolon" = 0xBA; "Quote" = 0xDE; "Backtick" = 0xC0;
    "ArrowLeft" = 0x25; "ArrowUp" = 0x26; "ArrowRight" = 0x27; "ArrowDown" = 0x28; "Backspace" = 0x08;
    "Delete" = 0x2E; "Enter" = 0x0D; "Space" = 0x20; "Escape" = 0x1B; "Home" = 0x24; "End" = 0x23;
    "PageUp" = 0x21; "PageDown" = 0x22; "Tab" = 0x09
}
function KeyCode([string]$name) {
    if ($Vk.ContainsKey($name)) { return [uint16]$Vk[$name] }
    if ($name -match '^F(\d+)$') { return [uint16](0x6F + [int]$Matches[1]) }
    if ($name -match '^Num(\d)$') { return [uint16](0x30 + [int]$Matches[1]) }
    if ($name.Length -eq 1) { return [uint16][char]$name.ToUpper() }
    throw "no key code for $name"
}
function Press([string]$chord) {
    $parts = $chord.Split('+'); $mods = @()
    foreach ($m in $parts[0..($parts.Length - 2)]) {
        if ($parts.Length -eq 1) { break }
        switch ($m) { "Cmd" { $mods += 0x11 } "Alt" { $mods += 0x12 } "Shift" { $mods += 0x10 } }
    }
    [Keys]::Chord([uint16[]]$mods, (KeyCode $parts[-1]))
}

$home_ = Join-Path $env:TEMP ("bae-sweep-" + [guid]::NewGuid())
New-Item -ItemType Directory $home_ | Out-Null
$log = Join-Path $home_ "actions.jsonl"; New-Item -ItemType File $log | Out-Null
$env:BAE_HOME = $home_; $env:BAE_ACTION_LOG = $log; $env:BAE_LANG = "en"
$proc = Start-Process -FilePath $Bae -PassThru
Start-Sleep 5
[Keys]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
$read = 0
function NewActions { $lines = @(Get-Content $log); $new = $lines[$script:read..($lines.Count - 1)] | Where-Object { $_ }; $script:read = $lines.Count; return @($new | ForEach-Object { $_ | ConvertFrom-Json }) }

# A comp with a solid, selected.
foreach ($c in @("Cmd+N", "Cmd+Y")) { Press $c; Start-Sleep 1; Press "Enter"; Start-Sleep 1 }
NewActions | Out-Null

if (-not $Table) {
    # The catalog's key table, generated now (not kept in the repository).
    $Table = Join-Path ([IO.Path]::GetTempPath()) "bae-ae-shortcuts.json"
    $env:BAE_SHORTCUTS_JSON = $Table
    cargo test -q -p bae-ui-motion --lib shortcuts_json | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "generating the key table failed" }
}
$rows = (Get-Content $Table -Raw | ConvertFrom-Json).rows | Where-Object { -not $_.native -and -not $_.shadowed -and $_.own_panel -and $_.windows.Count -gt 0 }
$working = @($rows | Where-Object { $_.works }); if ($Limit -gt 0) { $working = $working[0..($Limit - 1)] }
$passed = @(); $failed = @(); $skipped = @(); $notCovered = @()
foreach ($r in $working) {
    $what = "``$($r.context)/$($r.command)`` -> ``$($r.action)``"
    if ($Skip.ContainsKey($r.action)) { $skipped += "${what}: $($Skip[$r.action])"; continue }
    if ($r.scope) { $notCovered += "$what ($($r.scope) panel; the CI sweep covers it)"; continue }
    foreach ($chord in $r.windows) {
        Press $chord; Start-Sleep -Milliseconds ([int]($Settle * 1000))
        $ran = NewActions
        $hits = @($ran | Where-Object { $_.id -eq $r.action -and ($null -eq $r.params -or (($_.params | ConvertTo-Json -Compress) -eq ($r.params | ConvertTo-Json -Compress))) })
        if ($hits.Count -eq 1) { $passed += "$what ``$chord``" } else { $failed += "$what ``$chord``: ran $($hits.Count) times (dispatched: $((@($ran | ForEach-Object { $_.id }) -join ', ')))" }
        Press "Escape"; Start-Sleep -Milliseconds 200; Press "Escape"; Start-Sleep -Milliseconds 200; NewActions | Out-Null
        if ($proc.HasExited) { $failed += "$what ``$chord``: BAE exited"; break }
    }
}
if (-not $proc.HasExited) { Stop-Process $proc }

$date = Get-Date -Format "yyyy-MM-dd"
$out = @("# AE 快捷键真机扫描（Windows，$date）", "",
    "- 已实现：$($working.Count) 条；真机按键通过 $($passed.Count)，失败 $($failed.Count)",
    "- 跳过：$($skipped.Count)；面板作用域（CI 扫描覆盖）：$($notCovered.Count)", "")
foreach ($sec in @(@("失败", $failed), @("跳过", $skipped), @("面板作用域", $notCovered), @("通过", $passed))) {
    $out += "## $($sec[0])（$($sec[1].Count)）"; $out += ""; $out += @($sec[1] | ForEach-Object { "- $_" }); $out += ""
}
if ($Report) { New-Item -ItemType Directory -Force (Split-Path $Report) | Out-Null; $out | Set-Content -Encoding utf8 $Report }
$out | Select-Object -First 40
if ($failed.Count -gt 0) { exit 1 }
