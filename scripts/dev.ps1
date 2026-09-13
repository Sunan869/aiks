# scripts/dev.ps1
#
# Starts AIKS Desktop in Tauri development mode.
# Compatible with: Windows PowerShell 5.1 and PowerShell 7+
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\dev.ps1

$ErrorActionPreference = "Stop"

$ScriptDir   = $PSScriptRoot
$ProjectRoot = Split-Path $ScriptDir -Parent

$RuntimeDest = Join-Path $ProjectRoot "apps\aiks-desktop\src-tauri\resources\siyuan"
$KernelExe   = Join-Path $RuntimeDest "kernel\SiYuan-Kernel.exe"
$DesktopDir  = Join-Path $ProjectRoot "apps\aiks-desktop"

Write-Host "=== AIKS Desktop Dev Mode ===" -ForegroundColor Cyan
Write-Host ""

# ── Step 1: Check runtime ─────────────────────────────────────────────────────
Write-Host "Checking embedded SiYuan runtime..."

$runtimeReady = $false
if (Test-Path $KernelExe) {
    $kernelSize = (Get-Item $KernelExe).Length
    $stageOk    = (Get-ChildItem (Join-Path $RuntimeDest "stage") -Recurse -File -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0
    $appearOk   = (Get-ChildItem (Join-Path $RuntimeDest "appearance") -Recurse -File -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0

    if ($kernelSize -gt 1MB -and $stageOk -and $appearOk) {
        $runtimeReady = $true
        Write-Host "  Runtime: OK  ($([math]::Round($kernelSize/1MB,1)) MB kernel, stage, appearance)" -ForegroundColor Green
    } else {
        Write-Host "  Runtime: incomplete (kernel=$kernelSize bytes, stage=$stageOk, appearance=$appearOk)" -ForegroundColor Yellow
    }
} else {
    Write-Host "  Runtime: NOT FOUND" -ForegroundColor Yellow
}

if (-not $runtimeReady) {
    Write-Host ""
    Write-Host "SiYuan runtime is not ready. Running setup..." -ForegroundColor Yellow
    Write-Host ""
    & powershell -ExecutionPolicy Bypass -File (Join-Path $ScriptDir "setup-siyuan.ps1")
    if ($LASTEXITCODE -ne 0) {
        Write-Error "setup-siyuan.ps1 failed. Cannot start dev mode."
    }
    Write-Host ""
}

# ── Step 2: Verify npm dependencies ──────────────────────────────────────────
Write-Host "Checking npm dependencies..."
$nodeModules = Join-Path $DesktopDir "node_modules"
if (-not (Test-Path $nodeModules)) {
    Write-Host "  Installing npm packages..."
    Push-Location $DesktopDir
    npm install
    if ($LASTEXITCODE -ne 0) { Write-Error "npm install failed" }
    Pop-Location
} else {
    Write-Host "  node_modules: OK" -ForegroundColor Green
}

# ── Step 3: Start Tauri dev ───────────────────────────────────────────────────
Write-Host ""
Write-Host "Starting AIKS Desktop (dev mode)..."
Write-Host "  Control Center: http://localhost:1420"
Write-Host "  SiYuan runtime will start automatically on first run"
Write-Host "  Press Ctrl+C to stop"
Write-Host ""

Push-Location $DesktopDir
npx tauri dev
Pop-Location
