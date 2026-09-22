# Run in a disposable PowerShell process. All external commands are test doubles.
$ErrorActionPreference = "Stop"
$source = Join-Path (Split-Path $PSScriptRoot -Parent) "scripts\dev.ps1"
$root = Join-Path ([IO.Path]::GetTempPath()) ("aiks-dev-test-" + [Guid]::NewGuid().ToString("N"))
$priorMode = $env:AIKS_BACKEND_MODE
$priorLocation = (Get-Location).Path
$script:events = New-Object System.Collections.Generic.List[string]
$script:cargoFailure = $false
$script:npmFailure = $false
function Assert-True($condition, $message) { if (-not $condition) { throw $message } }
try {
    $scripts = Join-Path $root "scripts"
    $desktop = Join-Path $root "apps\aiks-desktop"
    $kernel = Join-Path $desktop "src-tauri\resources\siyuan\kernel"
    New-Item -ItemType Directory -Path $scripts, $kernel -Force | Out-Null
    Copy-Item -LiteralPath $source -Destination (Join-Path $scripts "dev.ps1")
    Set-Content -LiteralPath (Join-Path $scripts "setup-siyuan.ps1") -Value "# Test-only resource setup"
    Set-Content -LiteralPath (Join-Path $kernel "SiYuan-Kernel.exe") -Value "not executable: test resource"
    function global:powershell { $script:events.Add("setup"); $global:LASTEXITCODE = 0 }
    function global:npm { $script:events.Add("npm:" + ($args -join " ")); $global:LASTEXITCODE = $(if ($script:npmFailure) { 1 } else { 0 }) }
    function global:cargo { $script:events.Add("cargo:" + ($args -join " ")); $global:LASTEXITCODE = $(if ($script:cargoFailure) { 1 } else { 0 }) }
    function global:npx {
        $script:events.Add("npx:" + ($args -join " ") + ":" + $env:AIKS_BACKEND_MODE)
        Assert-True ((Get-Location).Path -eq $desktop) "Tauri must run from the Desktop folder"
        $global:LASTEXITCODE = 0
    }
    $entry = Join-Path $scripts "dev.ps1"
    $env:AIKS_BACKEND_MODE = "sentinel-original"
    & $entry
    Assert-True (($script:events -join "|") -eq "setup|npm:ci|cargo:build --locked -p aiks-service|npx:tauri dev:service_local") "Service one-command sequence changed"
    Assert-True ($env:AIKS_BACKEND_MODE -eq "sentinel-original") "Environment was not restored"
    Assert-True ((Get-Location).Path -eq $priorLocation) "Working directory was not restored"

    New-Item -ItemType Directory -Path (Join-Path $desktop "node_modules") | Out-Null
    $script:events.Clear()
    & $entry -Legacy
    Assert-True (($script:events -join "|") -eq "setup|npx:tauri dev:legacy") "Legacy must not build/start Service"
    Assert-True ($env:AIKS_BACKEND_MODE -eq "sentinel-original") "Legacy environment was not restored"

    $script:events.Clear(); $script:cargoFailure = $true; $failed = $false
    try { & $entry } catch { $failed = $true }
    Assert-True $failed "A failed Service build must fail startup"
    Assert-True (-not ($script:events -match "^npx:")) "Must not start a stale binary after failed build"
    Assert-True ($env:AIKS_BACKEND_MODE -eq "sentinel-original") "Failed build leaked its mode"
    Assert-True ((Get-Location).Path -eq $priorLocation) "Failed build leaked its working directory"
    Write-Host "PASS: Service orchestration, Legacy opt-out, failed-build stop, directory/environment restoration"
} finally {
    foreach ($name in @("powershell", "npm", "cargo", "npx")) { Remove-Item ("Function:global:" + $name) -ErrorAction SilentlyContinue }
    $env:AIKS_BACKEND_MODE = $priorMode
    Set-Location $priorLocation
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
