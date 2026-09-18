# scripts/build-release.ps1
#
# Complete AIKS Desktop release build pipeline.
# Compatible with: Windows PowerShell 5.1 and PowerShell 7+
#
# Requirements:
#   - Rust / cargo
#   - Node.js / npm / npx
#   - SiYuan runtime (setup-siyuan.ps1 will be invoked automatically if needed)
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1

$ErrorActionPreference = "Stop"

$ScriptDir       = $PSScriptRoot
$ProjectRoot     = Split-Path $ScriptDir -Parent
$VersionFile     = Join-Path $ProjectRoot "siyuan.version"
$RuntimeDest     = Join-Path $ProjectRoot "apps\aiks-desktop\src-tauri\resources\siyuan"
$KernelExe       = Join-Path $RuntimeDest "kernel\SiYuan-Kernel.exe"
$RuntimeMark     = Join-Path $RuntimeDest "aiks-runtime-version.txt"
$RuntimeManifest = Join-Path $RuntimeDest "aiks-runtime.json"
$DesktopDir      = Join-Path $ProjectRoot "apps\aiks-desktop"
$SetupScript     = Join-Path $ScriptDir "setup-siyuan.ps1"

Write-Host "=== AIKS Desktop Release Build ===" -ForegroundColor Cyan
Write-Host ""

# -----------------------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------------------

function Read-VersionFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "siyuan.version not found: $Path"
    }

    $cfg = @{}

    foreach ($rawLine in Get-Content -LiteralPath $Path) {
        $line = $rawLine.Trim()

        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }

        if ($line.StartsWith("#")) {
            continue
        }

        $index = $line.IndexOf("=")
        if ($index -le 0) {
            continue
        }

        $key = $line.Substring(0, $index).Trim()
        $value = $line.Substring($index + 1).Trim()

        if ($value.Length -ge 2) {
            if (
                ($value.StartsWith('"') -and $value.EndsWith('"')) -or
                ($value.StartsWith("'") -and $value.EndsWith("'"))
            ) {
                $value = $value.Substring(1, $value.Length - 2)
            }
        }

        if (-not [string]::IsNullOrWhiteSpace($key)) {
            $cfg[$key] = $value
        }
    }

    foreach ($required in @(
        "version",
        "workbench_version",
        "release_repo",
        "release_tag",
        "asset",
        "platform",
        "sha256",
        "upstream_commit",
        "fork_commit",
        "profile",
        "bridge_protocol"
    )) {
        if (
            -not $cfg.ContainsKey($required) -or
            [string]::IsNullOrWhiteSpace([string]$cfg[$required])
        ) {
            throw "siyuan.version is missing required field: $required"
        }
    }

    return $cfg
}

function Assert-Command {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $Name"
    }
}

function Invoke-NativeCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [string[]]$Arguments = @(),

        [Parameter(Mandatory = $true)]
        [string]$FailureMessage
    )

    # Windows PowerShell 5.1 converts native-process stderr into ErrorRecord
    # objects. With the script-wide ErrorActionPreference=Stop, harmless
    # compiler warnings can otherwise terminate the build.
    $previousErrorActionPreference = $ErrorActionPreference
    $exitCode = $null

    try {
        $ErrorActionPreference = "Continue"
        & $FilePath @Arguments
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }

    if ($null -eq $exitCode) {
        throw "$FailureMessage (no process exit code was returned)"
    }

    if ($exitCode -ne 0) {
        throw "$FailureMessage (exit code: $exitCode)"
    }
}

function Get-FileCount {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return 0
    }

    return (Get-ChildItem -LiteralPath $Path -Recurse -File -ErrorAction SilentlyContinue | Measure-Object).Count
}

function Assert-RuntimeManifestIdentity {
    param(
        [Parameter(Mandatory = $true)]
        $Manifest,

        [Parameter(Mandatory = $true)]
        [hashtable]$Config
    )

    $expected = [ordered]@{
        workbenchVersion = [string]$Config["workbench_version"]
        siyuanBaseVersion = [string]$Config["version"]
        upstreamCommit = [string]$Config["upstream_commit"]
        forkRepository = [string]$Config["release_repo"]
        forkCommit = [string]$Config["fork_commit"]
        profile = [string]$Config["profile"]
        platform = [string]$Config["platform"]
        bridgeProtocol = [int]$Config["bridge_protocol"]
    }

    foreach ($entry in $expected.GetEnumerator()) {
        $actual = $Manifest.($entry.Key)
        if ([string]$actual -ne [string]$entry.Value) {
            throw "Runtime manifest mismatch for '$($entry.Key)': expected '$($entry.Value)', got '$actual'"
        }
    }
}

function Test-SiyuanRuntime {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ExpectedVersion,

        [Parameter(Mandatory = $true)]
        [hashtable]$Config
    )

    if (-not (Test-Path -LiteralPath $KernelExe)) {
        return $false
    }

    $kernelItem = Get-Item -LiteralPath $KernelExe
    if ($kernelItem.Length -le 1MB) {
        return $false
    }

    if ((Get-FileCount (Join-Path $RuntimeDest "stage")) -le 0) {
        return $false
    }

    if ((Get-FileCount (Join-Path $RuntimeDest "appearance")) -le 0) {
        return $false
    }

    if (-not (Test-Path -LiteralPath $RuntimeMark)) {
        return $false
    }

    $actualVersion = (Get-Content -LiteralPath $RuntimeMark -Raw).Trim()
    if ($actualVersion -ne $ExpectedVersion) {
        return $false
    }

    if (-not (Test-Path -LiteralPath $RuntimeManifest)) {
        return $false
    }

    try {
        $manifest = Get-Content -LiteralPath $RuntimeManifest -Raw | ConvertFrom-Json
        Assert-RuntimeManifestIdentity -Manifest $manifest -Config $Config
    }
    catch {
        return $false
    }

    return $true
}

# -----------------------------------------------------------------------------
# Read version lock
# -----------------------------------------------------------------------------

$cfg = Read-VersionFile -Path $VersionFile

$SiyuanVersion = [string]$cfg["version"]
$ReleaseTag     = [string]$cfg["release_tag"]
$AssetName      = [string]$cfg["asset"]
$Platform       = [string]$cfg["platform"]

Write-Host "Version lock:"
Write-Host "  SiYuan version : $SiyuanVersion"
Write-Host "  Release tag    : $ReleaseTag"
Write-Host "  Asset          : $AssetName"
Write-Host "  Platform       : $Platform"
Write-Host ""

# -----------------------------------------------------------------------------
# Step 1: Check / prepare runtime
# -----------------------------------------------------------------------------

Write-Host "Step 1/7: Checking SiYuan runtime..."

$runtimeReady = Test-SiyuanRuntime -ExpectedVersion $SiyuanVersion -Config $cfg

if ($runtimeReady) {
    $kernelSize = (Get-Item -LiteralPath $KernelExe).Length
    Write-Host "  Runtime OK ($([math]::Round($kernelSize / 1MB, 1)) MB, version $SiyuanVersion, locked identity verified)" -ForegroundColor Green
}
else {
    Write-Host "  Runtime not ready or locked identity mismatched. Running setup-siyuan.ps1..." -ForegroundColor Yellow

    if (-not (Test-Path -LiteralPath $SetupScript)) {
        throw "setup-siyuan.ps1 not found: $SetupScript"
    }

    & $SetupScript

    Write-Host ""
    Write-Host "  Re-checking runtime..." -ForegroundColor Yellow

    if (-not (Test-SiyuanRuntime -ExpectedVersion $SiyuanVersion -Config $cfg)) {
        throw "Runtime is still invalid after setup-siyuan.ps1."
    }

    Write-Host "  Runtime preparation OK" -ForegroundColor Green
}

# -----------------------------------------------------------------------------
# Step 2: Validate runtime
# -----------------------------------------------------------------------------

Write-Host ""
Write-Host "Step 2/7: Runtime validation..."

$manifest = Get-Content -LiteralPath $RuntimeManifest -Raw | ConvertFrom-Json
Assert-RuntimeManifestIdentity -Manifest $manifest -Config $cfg

$kernelSizeMB = [math]::Round((Get-Item -LiteralPath $KernelExe).Length / 1MB, 1)
$stageCount = Get-FileCount (Join-Path $RuntimeDest "stage")
$appearanceCount = Get-FileCount (Join-Path $RuntimeDest "appearance")
$guideCount = Get-FileCount (Join-Path $RuntimeDest "guide")

Write-Host "  Manifest   : locked identity OK" -ForegroundColor Green
Write-Host "  Kernel     : $kernelSizeMB MB  OK" -ForegroundColor Green
Write-Host "  Stage      : $stageCount files  OK" -ForegroundColor Green
Write-Host "  Appearance : $appearanceCount files  OK" -ForegroundColor Green

if ($guideCount -gt 0) {
    Write-Host "  Guide      : $guideCount files  OK" -ForegroundColor Green
}
else {
    Write-Host "  Guide      : not present / empty" -ForegroundColor Yellow
}

Write-Host "  Testing kernel command: serve --help"

Invoke-NativeCommand `
    -FilePath $KernelExe `
    -Arguments @("serve", "--help") `
    -FailureMessage "SiYuan Kernel serve --help smoke test failed"

Write-Host "  Kernel smoke test OK" -ForegroundColor Green

# -----------------------------------------------------------------------------
# Step 3: Run Rust tests
# -----------------------------------------------------------------------------

Write-Host ""
Write-Host "Step 3/7: Running aiks-core tests..."

Assert-Command "cargo"

Push-Location $ProjectRoot
try {
    Invoke-NativeCommand `
        -FilePath "cargo" `
        -Arguments @("test", "-p", "aiks-core", "--lib", "--quiet") `
        -FailureMessage "aiks-core tests FAILED"
}
finally {
    Pop-Location
}

Write-Host "  All tests passed" -ForegroundColor Green

# -----------------------------------------------------------------------------
# Step 4: Install npm dependencies
# -----------------------------------------------------------------------------

Write-Host ""
Write-Host "Step 4/7: Installing npm dependencies..."

Assert-Command "npm"
Assert-Command "npx"

Push-Location $DesktopDir
try {
    Invoke-NativeCommand `
        -FilePath "npm" `
        -Arguments @("ci", "--quiet") `
        -FailureMessage "npm ci failed"
}
finally {
    Pop-Location
}

Write-Host "  npm: OK" -ForegroundColor Green

# -----------------------------------------------------------------------------
# Step 5: Validate Tauri bundle resources
# -----------------------------------------------------------------------------

Write-Host ""
Write-Host "Step 5/7: Validating Tauri bundle resources..."

foreach ($requiredPath in @(
    (Join-Path $RuntimeDest "kernel\SiYuan-Kernel.exe"),
    (Join-Path $RuntimeDest "stage"),
    (Join-Path $RuntimeDest "appearance"),
    $RuntimeManifest
)) {
    if (-not (Test-Path -LiteralPath $requiredPath)) {
        throw "Required Tauri bundle resource missing: $requiredPath"
    }
}

Write-Host "  resources/siyuan/kernel/SiYuan-Kernel.exe  OK" -ForegroundColor Green
Write-Host "  resources/siyuan/stage/                    OK" -ForegroundColor Green
Write-Host "  resources/siyuan/appearance/               OK" -ForegroundColor Green
Write-Host "  resources/siyuan/aiks-runtime.json         OK" -ForegroundColor Green

if (Test-Path -LiteralPath (Join-Path $RuntimeDest "guide")) {
    Write-Host "  resources/siyuan/guide/                    OK" -ForegroundColor Green
}

# -----------------------------------------------------------------------------
# Step 6: Build Tauri
# -----------------------------------------------------------------------------

Write-Host ""
Write-Host "Step 6/7: Building Tauri release package..."
Write-Host "  (This may take several minutes on first build)"

Push-Location $DesktopDir
try {
    Invoke-NativeCommand `
        -FilePath "npx" `
        -Arguments @("tauri", "build") `
        -FailureMessage "Tauri build FAILED"
}
finally {
    Pop-Location
}

Write-Host "  Tauri build OK" -ForegroundColor Green

# -----------------------------------------------------------------------------
# Step 7: Find and verify installer
# -----------------------------------------------------------------------------

Write-Host ""
Write-Host "Step 7/7: Verifying output..."

# In a Cargo workspace Tauri normally uses the workspace-level target directory.
# Keep the src-tauri target directory as a fallback for non-workspace layouts.
$bundleCandidates = @(
    (Join-Path $ProjectRoot "target\release\bundle"),
    (Join-Path $DesktopDir "src-tauri\target\release\bundle")
)

$installer = $null

foreach ($bundleRoot in $bundleCandidates) {
    if (-not (Test-Path -LiteralPath $bundleRoot)) {
        continue
    }

    $nsisDir = Join-Path $bundleRoot "nsis"
    if (Test-Path -LiteralPath $nsisDir) {
        $installer = Get-ChildItem -LiteralPath $nsisDir -Filter "*.exe" -File |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1

        if ($installer) {
            break
        }
    }

    $msiDir = Join-Path $bundleRoot "msi"
    if (Test-Path -LiteralPath $msiDir) {
        $installer = Get-ChildItem -LiteralPath $msiDir -Filter "*.msi" -File |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1

        if ($installer) {
            break
        }
    }
}

if (-not $installer) {
    Write-Host ""
    Write-Host "Checked bundle locations:" -ForegroundColor Yellow
    foreach ($bundleRoot in $bundleCandidates) {
        Write-Host "  $bundleRoot"
    }

    throw "Tauri reported success, but no NSIS EXE or MSI installer was found."
}

$sizeMB = [math]::Round($installer.Length / 1MB, 1)

Write-Host "  Installer: $($installer.Name) ($sizeMB MB)" -ForegroundColor Green
Write-Host "  Location : $($installer.FullName)"

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------

Write-Host ""
Write-Host "=== Build Complete ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "  AIKS Version   : 0.2.0"
Write-Host "  SiYuan Version : $SiyuanVersion (bundled)"
Write-Host "  Installer      : $($installer.FullName)" -ForegroundColor Green
Write-Host ""
Write-Host "  NOTE: The installer bundles SiYuan $SiyuanVersion (AGPL-3.0)."
Write-Host "  Ensure THIRD_PARTY_NOTICES.md and applicable license/source notices are included."
Write-Host ""
