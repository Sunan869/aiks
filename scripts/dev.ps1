# scripts/dev.ps1
# Starts the complete local Service desktop development workflow.
# Use -Legacy to run the original desktop without adopting or deleting any data.
param([switch]$Legacy)

$ErrorActionPreference = "Stop"
$ScriptDir = $PSScriptRoot
$ProjectRoot = Split-Path $ScriptDir -Parent
$RuntimeDest = Join-Path $ProjectRoot "apps\aiks-desktop\src-tauri\resources\siyuan"
$KernelExe = Join-Path $RuntimeDest "kernel\SiYuan-Kernel.exe"
$DesktopDir = Join-Path $ProjectRoot "apps\aiks-desktop"
$SetupScript = Join-Path $ScriptDir "setup-siyuan.ps1"

Write-Host "=== AIKS Desktop Dev Mode ===" -ForegroundColor Cyan
Write-Host "Checking customized SiYuan runtime identity..."
if (-not (Test-Path -LiteralPath $SetupScript)) { throw "setup-siyuan.ps1 not found" }
& powershell -ExecutionPolicy Bypass -File $SetupScript
if ($LASTEXITCODE -ne 0) { throw "setup-siyuan.ps1 failed" }
if (-not (Test-Path -LiteralPath $KernelExe)) { throw "Customized SiYuan runtime is missing" }

$nodeModules = Join-Path $DesktopDir "node_modules"
if (-not (Test-Path $nodeModules)) {
    Push-Location $DesktopDir
    try {
        npm ci
        if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
    } finally { Pop-Location }
}

$PreviousMode = $env:AIKS_BACKEND_MODE
try {
    if ($Legacy) { $env:AIKS_BACKEND_MODE = "legacy" }
    else {
        $env:AIKS_BACKEND_MODE = "service_local"
        Write-Host "Building the local AIKS Service..." -ForegroundColor Cyan
        Push-Location $ProjectRoot
        try {
            cargo build --locked -p aiks-service
            if ($LASTEXITCODE -ne 0) { throw "AIKS Service build failed" }
        } finally { Pop-Location }
    }
    Write-Host "Starting Desktop; the native controller owns Service and SiYuan." -ForegroundColor Green
    Push-Location $DesktopDir
    try {
        npx tauri dev
        if ($LASTEXITCODE -ne 0) { throw "Tauri development process failed" }
    } finally { Pop-Location }
} finally {
    $env:AIKS_BACKEND_MODE = $PreviousMode
}
