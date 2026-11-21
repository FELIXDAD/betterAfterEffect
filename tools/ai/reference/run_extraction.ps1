<#
.SYNOPSIS
    Run the full Adobe Illustrator reference data extraction pipeline on Windows.
.DESCRIPTION
    Connects to Adobe Illustrator via COM, runs the ExtendScript DOM/catalog extractor,
    reads the main window menu tree without GUI clicks, and builds structured JSON/Markdown
    reference files under docs/reference/ai/.
#>
param(
    [string]$InstallDir = "C:\Program Files\Adobe\Adobe Illustrator 2026",
    [string]$TempDir = "E:\bae-test\ref-ai",
    [string]$OutputDir = "",
    [switch]$UseKeyboardMenus,
    [switch]$KeepTemp
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $repoRoot = (Resolve-Path (Join-Path $scriptDir "..\..\..")).Path
    $OutputDir = Join-Path $repoRoot "docs\reference\ai"
}

New-Item -ItemType Directory -Force $TempDir | Out-Null
$domRawPath = Join-Path $TempDir "dom_and_catalogs_raw.json"
$menusRawPath = Join-Path $TempDir "menus_raw.json"
$jsxPath = Join-Path $scriptDir "extract_ai_dom_and_catalogs.jsx"
$menusScriptPath = Join-Path $scriptDir $(if ($UseKeyboardMenus) { "extract_ai_menus_keyboard.ps1" } else { "extract_ai_menus.ps1" })
$builderPath = Join-Path $scriptDir "build_ai_reference.py"

Write-Host "[1/4] Connecting to Illustrator via COM..."
$ai = New-Object -ComObject Illustrator.Application
Start-Sleep -Seconds 2

Write-Host "[2/4] Running ExtendScript DOM and catalog extraction..."
$escapedOut = $domRawPath.Replace("\", "\\")
$ai.DoJavaScript("var __BAE_OUTPUT_PATH__ = `"$escapedOut`";") | Out-Null
$jsxResult = $ai.DoJavaScriptFile($jsxPath)
Write-Host "ExtendScript result: $jsxResult"

if ($UseKeyboardMenus) {
    Write-Host "[3/4] Extracting Illustrator menu tree via keyboard Alt+key / MN_GETHMENU..."
} else {
    Write-Host "[3/4] Extracting Illustrator menu tree (read-only)..."
}
powershell -NoProfile -ExecutionPolicy Bypass -File $menusScriptPath -OutputPath $menusRawPath -InstallDir $InstallDir
if ($LASTEXITCODE -ne 0) {
    throw "$menusScriptPath failed with exit code $LASTEXITCODE"
}

Write-Host "[4/4] Building structured reference files in $OutputDir..."
python $builderPath --install-dir $InstallDir --menus-raw $menusRawPath --dom-raw $domRawPath --output-dir $OutputDir
if ($LASTEXITCODE -ne 0) {
    throw "build_ai_reference.py failed with exit code $LASTEXITCODE"
}

if (-not $KeepTemp) {
    Remove-Item -Force -ErrorAction SilentlyContinue $domRawPath, $menusRawPath
}

Write-Host "Done."
