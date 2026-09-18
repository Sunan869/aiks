# scripts/setup-siyuan.ps1
#
# Prepares the customized AIKS SiYuan runtime.
# The runtime must be built from Sunan869/aiks-siyuan and published as a ZIP.
# Compatible with Windows PowerShell 5.1 and PowerShell 7+.

param(
    [switch]$Force,
    [string]$RuntimeArchive
)

$ErrorActionPreference = "Stop"

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch { }

$ScriptDir   = $PSScriptRoot
$ProjectRoot = Split-Path $ScriptDir -Parent
$VersionFile = Join-Path $ProjectRoot "siyuan.version"
$RuntimeDest = Join-Path $ProjectRoot "apps\aiks-desktop\src-tauri\resources\siyuan"
$CacheDir    = Join-Path $ProjectRoot ".build\cache\aiks-siyuan"
$StagingDir  = Join-Path $ProjectRoot ".build\staging\aiks-siyuan-runtime"

function Read-VersionFile {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "siyuan.version not found: $Path"
    }

    $cfg = @{}
    foreach ($rawLine in Get-Content -LiteralPath $Path) {
        $line = $rawLine.Trim()
        if ([string]::IsNullOrWhiteSpace($line) -or $line.StartsWith("#")) { continue }
        $index = $line.IndexOf("=")
        if ($index -le 0) { continue }
        $key = $line.Substring(0, $index).Trim()
        $value = $line.Substring($index + 1).Trim()
        if ($value.Length -ge 2) {
            if (($value.StartsWith('"') -and $value.EndsWith('"')) -or
                ($value.StartsWith("'") -and $value.EndsWith("'"))) {
                $value = $value.Substring(1, $value.Length - 2)
            }
        }
        if (-not [string]::IsNullOrWhiteSpace($key)) { $cfg[$key] = $value }
    }
    return $cfg
}

function Require-Config {
    param([hashtable]$Config, [string[]]$Keys)
    foreach ($key in $Keys) {
        if (-not $Config.ContainsKey($key) -or [string]::IsNullOrWhiteSpace([string]$Config[$key])) {
            throw "siyuan.version is missing required field: $key"
        }
    }
}

function Get-Sha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-ManifestIdentity {
    param(
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)][hashtable]$Config
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

function Get-FileCount {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return 0 }
    return (Get-ChildItem -LiteralPath $Path -Recurse -File -ErrorAction SilentlyContinue | Measure-Object).Count
}

function Assert-RuntimeLayout {
    param([Parameter(Mandatory = $true)][string]$Root)

    $kernel = Join-Path $Root "kernel\SiYuan-Kernel.exe"
    if (-not (Test-Path -LiteralPath $kernel)) { throw "Runtime kernel missing: $kernel" }
    if ((Get-Item -LiteralPath $kernel).Length -le 1MB) { throw "Runtime kernel is unexpectedly small" }

    foreach ($dir in @("stage", "appearance", "guide")) {
        $path = Join-Path $Root $dir
        if ((Get-FileCount $path) -le 0) { throw "Runtime directory missing or empty: $dir" }
    }

    $manifestPath = Join-Path $Root "aiks-runtime.json"
    if (-not (Test-Path -LiteralPath $manifestPath)) { throw "Runtime manifest missing: $manifestPath" }
}

function Test-InstalledRuntime {
    param([hashtable]$Config)
    try {
        Assert-RuntimeLayout -Root $RuntimeDest
        $manifest = Get-Content -LiteralPath (Join-Path $RuntimeDest "aiks-runtime.json") -Raw | ConvertFrom-Json
        Assert-ManifestIdentity -Manifest $manifest -Config $Config
        return $true
    } catch {
        return $false
    }
}

function Invoke-KernelSmokeTest {
    param([Parameter(Mandatory = $true)][string]$KernelPath)
    $previous = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        & $KernelPath serve --help | Out-Null
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }
    if ($exitCode -ne 0) { throw "SiYuan kernel smoke test failed with exit code $exitCode" }
}

$cfg = Read-VersionFile -Path $VersionFile
Require-Config -Config $cfg -Keys @(
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
)

if ([string]$cfg["release_repo"] -ne "Sunan869/aiks-siyuan") {
    throw "V4.2 runtime must come from Sunan869/aiks-siyuan; configured: $($cfg['release_repo'])"
}
if ([string]$cfg["profile"] -ne "aiks-embedded") {
    throw "V4.2 runtime profile must be aiks-embedded"
}

Write-Host "=== AIKS Customized SiYuan Runtime Setup ===" -ForegroundColor Cyan
Write-Host "  Workbench : $($cfg['workbench_version'])"
Write-Host "  SiYuan    : $($cfg['version'])"
Write-Host "  Fork      : $($cfg['release_repo'])@$($cfg['fork_commit'])"
Write-Host "  Upstream  : $($cfg['upstream_commit'])"
Write-Host "  Profile   : $($cfg['profile'])"
Write-Host "  Platform  : $($cfg['platform'])"
Write-Host ""

if (-not $Force -and (Test-InstalledRuntime -Config $cfg)) {
    Write-Host "Customized runtime is already installed and matches the lock file." -ForegroundColor Green
    exit 0
}

New-Item -ItemType Directory -Force -Path $CacheDir | Out-Null

$archivePath = $RuntimeArchive
if ([string]::IsNullOrWhiteSpace($archivePath)) {
    $archivePath = Join-Path $CacheDir ([string]$cfg["asset"])
    $cachedOk = (Test-Path -LiteralPath $archivePath) -and ((Get-Sha256 $archivePath) -eq ([string]$cfg["sha256"]).ToLowerInvariant())

    if (-not $cachedOk) {
        if (Test-Path -LiteralPath $archivePath) { Remove-Item -LiteralPath $archivePath -Force }

        $repo = [string]$cfg["release_repo"]
        $tag = [string]$cfg["release_tag"]
        $assetName = [string]$cfg["asset"]
        $apiUrl = "https://api.github.com/repos/$repo/releases/tags/$tag"
        $headers = @{
            "User-Agent" = "AIKS-Runtime-Setup/4.2"
            "Accept" = "application/vnd.github+json"
        }

        Write-Host "Resolving customized runtime release $repo / $tag..."
        $release = Invoke-RestMethod -Uri $apiUrl -Headers $headers -Method Get
        $asset = $release.assets | Where-Object { $_.name -eq $assetName } | Select-Object -First 1
        if (-not $asset) { throw "Runtime release asset not found: $assetName" }

        Write-Host "Downloading $assetName..."
        $client = New-Object System.Net.WebClient
        $client.Headers.Add("User-Agent", "AIKS-Runtime-Setup/4.2")
        $client.DownloadFile($asset.browser_download_url, $archivePath)
    }
}

if (-not (Test-Path -LiteralPath $archivePath)) { throw "Runtime archive not found: $archivePath" }

$actualHash = Get-Sha256 $archivePath
$expectedHash = ([string]$cfg["sha256"]).ToLowerInvariant()
if ($actualHash -ne $expectedHash) {
    throw "Runtime SHA256 mismatch. Expected $expectedHash, got $actualHash"
}
Write-Host "Runtime SHA256: OK" -ForegroundColor Green

if (Test-Path -LiteralPath $StagingDir) { Remove-Item -LiteralPath $StagingDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $StagingDir | Out-Null
Expand-Archive -LiteralPath $archivePath -DestinationPath $StagingDir -Force

$manifestFile = Get-ChildItem -LiteralPath $StagingDir -Filter "aiks-runtime.json" -File -Recurse | Select-Object -First 1
if (-not $manifestFile) { throw "aiks-runtime.json not found after extraction" }
$runtimeRoot = $manifestFile.Directory.FullName

Assert-RuntimeLayout -Root $runtimeRoot
$manifest = Get-Content -LiteralPath $manifestFile.FullName -Raw | ConvertFrom-Json
Assert-ManifestIdentity -Manifest $manifest -Config $cfg
Write-Host "Runtime manifest identity: OK" -ForegroundColor Green

New-Item -ItemType Directory -Force -Path $RuntimeDest | Out-Null

# The AIKS bridge is source-controlled under resources/siyuan/data. Preserve it.
foreach ($dir in @("kernel", "stage", "appearance", "guide")) {
    $target = Join-Path $RuntimeDest $dir
    if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Recurse -Force }
    Copy-Item -LiteralPath (Join-Path $runtimeRoot $dir) -Destination $target -Recurse -Force
}

Copy-Item -LiteralPath (Join-Path $runtimeRoot "aiks-runtime.json") -Destination (Join-Path $RuntimeDest "aiks-runtime.json") -Force
Set-Content -LiteralPath (Join-Path $RuntimeDest "aiks-runtime-version.txt") -Value ([string]$cfg["version"]) -Encoding ASCII

Assert-RuntimeLayout -Root $RuntimeDest
$installedManifest = Get-Content -LiteralPath (Join-Path $RuntimeDest "aiks-runtime.json") -Raw | ConvertFrom-Json
Assert-ManifestIdentity -Manifest $installedManifest -Config $cfg
Invoke-KernelSmokeTest -KernelPath (Join-Path $RuntimeDest "kernel\SiYuan-Kernel.exe")

Write-Host ""
Write-Host "Customized AIKS SiYuan runtime is ready." -ForegroundColor Green
Write-Host "  Runtime: $RuntimeDest"
Write-Host "  Bridge data preserved: $(Test-Path (Join-Path $RuntimeDest 'data\plugins\aiks-bridge'))"
