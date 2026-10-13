# scripts/dev.ps1
#
# Starts AIKS Desktop in Tauri development mode.
# Compatible with Windows PowerShell 5.1 and PowerShell 7+
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\dev.ps1

$ErrorActionPreference = "Stop"

$ScriptDir   = $PSScriptRoot
$ProjectRoot = Split-Path $ScriptDir -Parent

$RuntimeDest = Join-Path $ProjectRoot "apps\aiks-desktop\src-tauri\resources\siyuan"
$KernelExe   = Join-Path $RuntimeDest "kernel\SiYuan-Kernel.exe"
$DesktopDir  = Join-Path $ProjectRoot "apps\aiks-desktop"
$SetupScript = Join-Path $ScriptDir "setup-siyuan.ps1"

Write-Host "=== AIKS Desktop Dev Mode ===" -ForegroundColor Cyan
Write-Host ""

# ── Step 1: Ensure the exact pinned runtime is present ────────────────────────
# setup-siyuan.ps1 is idempotent: if kernel/layout/manifest already match the
# runtime lock it returns immediately without downloading or replacing files.
Write-Host "Checking customized SiYuan runtime identity..."

if (-not (Test-Path -LiteralPath $SetupScript)) {
    Write-Error "setup-siyuan.ps1 not found: $SetupScript"
}

& powershell -ExecutionPolicy Bypass -File $SetupScript
if ($LASTEXITCODE -ne 0) {
    Write-Error "setup-siyuan.ps1 failed. Cannot start dev mode."
}

if (-not (Test-Path -LiteralPath $KernelExe)) {
    Write-Error "Customized SiYuan runtime is still missing after setup: $KernelExe"
}

$kernelSize = (Get-Item -LiteralPath $KernelExe).Length
Write-Host "  Runtime identity: OK ($([math]::Round($kernelSize / 1MB, 1)) MB kernel)" -ForegroundColor Green
Write-Host ""

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
