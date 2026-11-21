param(
    [string]$AeExe = "C:\Program Files\Adobe\Adobe After Effects 2026\Support Files\AfterFX.exe",
    [string]$OutDir = "E:\bae-test\ref-ae\raw",
    [string]$MediaDir = "E:\bae-test\ref-ae",
    [string]$LockDir = "E:\bae-test\locks",
    [string]$Owner = "bae-ref-ae",
    [string]$SkipEffects = "",
    [switch]$OnlyEffects,
    [int]$StartIndex = 0,
    [int]$TimeoutSeconds = 300,
    [int]$StallSeconds = 20
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$extractJsx = Join-Path $scriptDir "extract_ae_data.jsx"
$quitJsx = Join-Path $scriptDir "quit_ae.jsx"
$menusPs1 = Join-Path $scriptDir "extract_ae_menus.ps1"

if (-not (Test-Path $AeExe)) {
    throw "AfterFX.exe not found at: $AeExe"
}

function Clear-AeCrashFlags {
    $regPath = "HKCU:\Software\Adobe\After Effects\26.0"
    if (Test-Path $regPath) {
        foreach ($prop in @("ForceQuitOccured", "AbortOccured", "CrashOccurred")) {
            Remove-ItemProperty -Path $regPath -Name $prop -ErrorAction SilentlyContinue
        }
    }
    $appStatesPath = "HKCU:\Software\Adobe\After Effects\AppStates"
    if (Test-Path $appStatesPath) {
        Get-ChildItem -Path $appStatesPath -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    }
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
New-Item -ItemType Directory -Force -Path $LockDir | Out-Null

$aeLockPath = Join-Path $LockDir "ae.lock"

$waitStart = Get-Date
while (Test-Path $aeLockPath) {
    if (((Get-Date) - $waitStart).TotalMinutes -ge 30) {
        throw "Timed out waiting for ae.lock."
    }
    Write-Output ("Waiting for ae.lock: " + (Get-Content $aeLockPath -Raw -ErrorAction SilentlyContinue).Trim())
    Start-Sleep -Seconds 15
}

$lockValue = "$Owner $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') Extracting AE reference data"
New-Item -ItemType File -Path $aeLockPath -Value $lockValue -ErrorAction Stop | Out-Null
Write-Output "Acquired ae.lock"

$forceStopped = $false
try {
    Clear-AeCrashFlags

    $donePath = Join-Path $OutDir "done.json"
    $curEffectPath = Join-Path $OutDir "current_effect.txt"
    if (Test-Path $donePath) { Remove-Item $donePath -Force }
    if (Test-Path $curEffectPath) { Remove-Item $curEffectPath -Force }

    $env:BAE_AE_REF_OUT = ($OutDir -replace "\\", "/")
    $env:BAE_AE_REF_MEDIA = ($MediaDir -replace "\\", "/")
    $env:BAE_AE_SKIP_EFFECTS = $SkipEffects
    $env:BAE_AE_ONLY_EFFECTS = if ($OnlyEffects) { "1" } else { "0" }
    $env:BAE_AE_START_INDEX = "$StartIndex"

    Write-Output "Launching After Effects with extract_ae_data.jsx..."
    $aeProc = Start-Process -FilePath $AeExe -ArgumentList @("-r", $extractJsx) -PassThru

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $lastCur = ""
    $lastChange = Get-Date
    $stalled = $false

    while (-not (Test-Path $donePath)) {
        if ((Get-Date) -gt $deadline) {
            Write-Output "Timed out waiting for done.json. Last effect: $lastCur"
            $stalled = $true
            break
        }
        if ($aeProc.HasExited -and -not (Get-Process -Name "AfterFX" -ErrorAction SilentlyContinue)) {
            throw "AfterFX exited unexpectedly before done.json was written. Last effect: $lastCur"
        }
        if (Test-Path $curEffectPath) {
            $cur = (Get-Content $curEffectPath -Raw -ErrorAction SilentlyContinue)
            if ($cur -ne $lastCur) {
                $lastCur = $cur
                $lastChange = Get-Date
            } elseif (((Get-Date) - $lastChange).TotalSeconds -ge $StallSeconds) {
                Write-Output "STALL DETECTED after ${StallSeconds}s on effect: $lastCur"
                $stalled = $true
                break
            }
        }
        Start-Sleep -Seconds 2
    }

    if (-not $stalled) {
        Start-Sleep -Seconds 2
        Write-Output "extract_ae_data.jsx completed:"
        Write-Output (Get-Content $donePath -Raw)

        if (-not $OnlyEffects) {
            Write-Output "Reading After Effects menus..."
            $menusOut = Join-Path $OutDir "menus_raw.json"
            try {
                & $menusPs1 -OutPath $menusOut
            } catch {
                Write-Output ("Menu extraction warning: " + $_.Exception.Message)
            }
        }
    }

    Write-Output "Quitting After Effects..."
    if (-not $stalled) {
        Start-Process -FilePath $AeExe -ArgumentList @("-r", $quitJsx) | Out-Null
        $quitDeadline = (Get-Date).AddSeconds(20)
        while ((Get-Process -Name "AfterFX" -ErrorAction SilentlyContinue) -and ((Get-Date) -lt $quitDeadline)) {
            Start-Sleep -Seconds 1
        }
    }
    $remaining = Get-Process -Name "AfterFX" -ErrorAction SilentlyContinue
    if ($remaining) {
        $forceStopped = $true
        $remaining | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 2
    }
    if ($forceStopped) {
        Clear-AeCrashFlags
    }
    Write-Output "After Effects exited."
    if ($stalled) {
        exit 2
    }
}
finally {
    Clear-AeCrashFlags
    if (Test-Path $aeLockPath) {
        Remove-Item $aeLockPath -Force -ErrorAction SilentlyContinue
        Write-Output "Released ae.lock"
    }
}
