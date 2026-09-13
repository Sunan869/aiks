# scripts/setup-siyuan.ps1
#
# Prepares the embedded SiYuan Runtime for AIKS Desktop.
# Compatible with: Windows PowerShell 5.1 and PowerShell 7+
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\setup-siyuan.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\setup-siyuan.ps1 -Force
#
# -Force: Re-prepare even if runtime already exists and is valid.

param(
    [switch]$Force
)

$ErrorActionPreference = "Stop"

# GitHub requires modern TLS. This keeps Windows PowerShell 5.1 compatible.
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch { }

# ── Paths ─────────────────────────────────────────────────────────────────────
$ScriptDir   = $PSScriptRoot
$ProjectRoot = Split-Path $ScriptDir -Parent

$VersionFile   = Join-Path $ProjectRoot "siyuan.version"
$RuntimeDest   = Join-Path $ProjectRoot "apps\aiks-desktop\src-tauri\resources\siyuan"
$CacheDir      = Join-Path $ProjectRoot ".build\cache\siyuan"

# ── Parse siyuan.version ──────────────────────────────────────────────────────
function Read-VersionFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "siyuan.version not found at: $Path"
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

        $separatorIndex = $line.IndexOf("=")
        if ($separatorIndex -le 0) {
            continue
        }

        $key = $line.Substring(0, $separatorIndex).Trim()
        $value = $line.Substring($separatorIndex + 1).Trim()

        # Accept key=value, key = value, key="value", key = "value"
        if ($value.Length -ge 2) {
            $first = $value.Substring(0, 1)
            $last  = $value.Substring($value.Length - 1, 1)
            if (($first -eq '"' -and $last -eq '"') -or ($first -eq "'" -and $last -eq "'")) {
                $value = $value.Substring(1, $value.Length - 2)
            }
        }

        if (-not [string]::IsNullOrWhiteSpace($key)) {
            $cfg[$key] = $value
        }
    }

    return $cfg
}

if (-not (Test-Path $VersionFile)) {
    Write-Error "siyuan.version not found at: $VersionFile"
}

$cfg = Read-VersionFile $VersionFile

foreach ($key in @("version","release_tag","asset","platform","sha256")) {
    if (-not $cfg.ContainsKey($key) -or [string]::IsNullOrWhiteSpace($cfg[$key])) {
        Write-Error "siyuan.version is missing required field: $key"
    }
}

$SiyuanVersion  = $cfg["version"]
$ReleaseTag     = $cfg["release_tag"]
$AssetName      = $cfg["asset"]
$ExpectedSha256 = $cfg["sha256"].ToLowerInvariant()
$RuntimeVersionMarker = Join-Path $RuntimeDest "aiks-runtime-version.txt"

Write-Host "=== AIKS Embedded SiYuan Setup ===" -ForegroundColor Cyan
Write-Host "  Version file: $VersionFile" -ForegroundColor DarkGray
Write-Host ""
Write-Host "  Version   : $SiyuanVersion"
Write-Host "  Asset     : $AssetName"
Write-Host "  Platform  : $($cfg['platform'])"
Write-Host ""

# ── Idempotency check ─────────────────────────────────────────────────────────
$KernelExe = Join-Path $RuntimeDest "kernel\SiYuan-Kernel.exe"

if (-not $Force -and (Test-Path $KernelExe)) {
    $stagePath      = Join-Path $RuntimeDest "stage"
    $appearancePath = Join-Path $RuntimeDest "appearance"
    $guidePath      = Join-Path $RuntimeDest "guide"

    $stageOk      = Test-Path $stagePath
    $appearanceOk = Test-Path $appearancePath
    $guideOk      = Test-Path $guidePath

    $markerOk = $false
    if (Test-Path $RuntimeVersionMarker) {
        $installedVersion = (Get-Content -LiteralPath $RuntimeVersionMarker -Raw).Trim()
        $markerOk = ($installedVersion -eq $SiyuanVersion)
    }

    $kernelSize = (Get-Item $KernelExe).Length
    $contentOk = $false
    if ($stageOk -and $appearanceOk -and $guideOk) {
        $stageCount = @(Get-ChildItem $stagePath -Recurse -File -ErrorAction SilentlyContinue).Count
        $appearanceCount = @(Get-ChildItem $appearancePath -Recurse -File -ErrorAction SilentlyContinue).Count
        $guideCount = @(Get-ChildItem $guidePath -Recurse -File -ErrorAction SilentlyContinue).Count
        $contentOk = ($stageCount -gt 0 -and $appearanceCount -gt 0 -and $guideCount -gt 0)
    }

    if ($markerOk -and $kernelSize -gt 1MB -and $contentOk) {
        Write-Host "SiYuan runtime $SiyuanVersion already ready. Use -Force to re-prepare." -ForegroundColor Green
        exit 0
    }

    Write-Host "Runtime exists but is incomplete or version-mismatched. Re-preparing..." -ForegroundColor Yellow
}

# ── Ensure cache directory ────────────────────────────────────────────────────
New-Item -ItemType Directory -Force -Path (Join-Path $CacheDir $SiyuanVersion) | Out-Null
$InstallerCache = Join-Path $CacheDir "$SiyuanVersion\$AssetName"

# ── Helper: compute SHA256 ────────────────────────────────────────────────────
function Get-Sha256 {
    param([string]$FilePath)
    $hash = Get-FileHash -Path $FilePath -Algorithm SHA256
    return $hash.Hash.ToLower()
}

# ── Download if not cached ────────────────────────────────────────────────────
$needDownload = $true

if (Test-Path $InstallerCache) {
    Write-Host "Found cached installer. Verifying SHA256..."
    $actualHash = Get-Sha256 $InstallerCache
    if ($actualHash -eq $ExpectedSha256) {
        Write-Host "  Cache SHA256: OK" -ForegroundColor Green
        $needDownload = $false
    } else {
        Write-Host "  Cache SHA256 mismatch. Re-downloading." -ForegroundColor Yellow
        Remove-Item $InstallerCache -Force
    }
}

if ($needDownload) {
    Write-Host ""
    Write-Host "Resolving GitHub Release asset..."

    $ApiUrl = "https://api.github.com/repos/siyuan-note/siyuan/releases/tags/$ReleaseTag"

    try {
        # Windows PowerShell 5.1 compatible Invoke-RestMethod
        $headers = @{
            "User-Agent" = "AIKS-Setup/1.0"
            "Accept"     = "application/vnd.github+json"
        }

        $release = Invoke-RestMethod -Uri $ApiUrl -Headers $headers -Method Get
    } catch {
        Write-Error "Failed to fetch GitHub release info for tag '$ReleaseTag': $_"
    }

    # Find the exact asset
    $asset = $release.assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1

    if (-not $asset) {
        Write-Error @"
SiYuan release asset not found:
  tag  = $ReleaseTag
  asset= $AssetName

Available assets:
$($release.assets | ForEach-Object { "  - $($_.name)" } | Out-String)
"@
    }

    $downloadUrl = $asset.browser_download_url
    Write-Host "  Found: $($asset.name) ($([math]::Round($asset.size / 1MB, 1)) MB)"
    Write-Host "  URL  : $downloadUrl"
    Write-Host ""
    Write-Host "Downloading SiYuan $SiyuanVersion... (this may take a while)"

    $webClient = New-Object System.Net.WebClient
    $webClient.Headers.Add("User-Agent", "AIKS-Setup/1.0")
    $webClient.DownloadFile($downloadUrl, $InstallerCache)

    Write-Host "Download complete. Verifying SHA256..."
    $actualHash = Get-Sha256 $InstallerCache
    if ($actualHash -ne $ExpectedSha256) {
        Remove-Item $InstallerCache -Force -ErrorAction SilentlyContinue
        Write-Error @"
SiYuan checksum verification FAILED.

  Expected : $ExpectedSha256
  Actual   : $actualHash

The downloaded file has been removed. Please check your network and try again.
"@
    }
    Write-Host "  SHA256: OK" -ForegroundColor Green
}

# ── Clean destination ─────────────────────────────────────────────────────────
if (Test-Path $RuntimeDest) {
    Remove-Item $RuntimeDest -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $RuntimeDest | Out-Null

# ── Extract Runtime from NSIS Installer ───────────────────────────────────────
$StagingDir = Join-Path $ProjectRoot ".build\staging\siyuan-$SiyuanVersion"
if (Test-Path $StagingDir) {
    Remove-Item $StagingDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $StagingDir | Out-Null

Write-Host ""
Write-Host "Extracting SiYuan runtime..."

# Detect 7-Zip
$sevenZip = $null
foreach ($p in @("7z", "C:\Program Files\7-Zip\7z.exe", "C:\Program Files (x86)\7-Zip\7z.exe")) {
    try {
        $result = & $p i 2>&1
        if ($LASTEXITCODE -eq 0 -or ($result -join "") -match "7-Zip") {
            $sevenZip = $p
            break
        }
    } catch { }
}

if ($sevenZip) {
    Write-Host "  Using 7-Zip to extract NSIS installer..."
    & $sevenZip x $InstallerCache -o"$StagingDir" -y | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  7-Zip extraction failed, falling back to NSIS silent install..." -ForegroundColor Yellow
        $sevenZip = $null
    }
}

if (-not $sevenZip) {
    Write-Host "  Using NSIS silent install to staging directory..."
    Write-Host "  (This installs SiYuan temporarily to extract runtime files)"
    Write-Host "  Target: $StagingDir"

    # NSIS /S for silent, /D= for custom directory (MUST be last arg)
    $proc = Start-Process -FilePath $InstallerCache `
        -ArgumentList "/S /D=$StagingDir" `
        -Wait -PassThru -NoNewWindow

    if ($proc.ExitCode -ne 0) {
        Write-Error "NSIS silent install failed with exit code: $($proc.ExitCode)"
    }
    Write-Host "  NSIS install complete."
}

# ── Locate SiYuan resources inside staging ────────────────────────────────────
function Find-SiyuanResources {
    param([string]$Root)

    # Try common layouts:
    # Layout 1 (7z extract): $INSTDIR\resources\kernel\, $INSTDIR\resources\stage\
    # Layout 2 (NSIS install): $INSTDIR\resources\...
    # Layout 3 (flat): $INSTDIR\kernel\, $INSTDIR\stage\

    $candidates = @(
        (Join-Path $Root "resources"),
        (Join-Path $Root '$INSTDIR\resources'),
        $Root
    )

    foreach ($candidate in $candidates) {
        $kernelPath = Join-Path $candidate "kernel\SiYuan-Kernel.exe"
        if (Test-Path $kernelPath) {
            return $candidate
        }
    }

    # Deep search
    $found = Get-ChildItem -Path $Root -Recurse -Filter "SiYuan-Kernel.exe" -ErrorAction SilentlyContinue |
             Select-Object -First 1
    if ($found) {
        return $found.DirectoryName | Split-Path -Parent
    }

    return $null
}

$resourceRoot = Find-SiyuanResources $StagingDir

if (-not $resourceRoot) {
    # List what we got
    Write-Host "Staging directory contents:" -ForegroundColor Yellow
    Get-ChildItem $StagingDir -Recurse -Depth 3 | Select-Object FullName | ForEach-Object {
        Write-Host "  $($_.FullName.Replace($StagingDir,''))"
    }
    Write-Error "Could not locate SiYuan-Kernel.exe in staging directory: $StagingDir"
}

Write-Host "  Resource root: $resourceRoot"

# ── Copy Runtime to destination ───────────────────────────────────────────────
Write-Host ""
Write-Host "Copying runtime to destination..."

$requiredDirs = @("kernel", "stage", "appearance", "guide")

foreach ($dir in $requiredDirs) {
    $src = Join-Path $resourceRoot $dir
    $dst = Join-Path $RuntimeDest $dir

    if (Test-Path $src) {
        Copy-Item -Path $src -Destination $dst -Recurse -Force
        $fileCount = (Get-ChildItem $dst -Recurse -File).Count
        Write-Host ("  {0,-12} OK  ({1} files)" -f $dir, $fileCount) -ForegroundColor Green
    } else {
        Write-Host ("  {0,-12} NOT FOUND in staging" -f $dir) -ForegroundColor Red
        Write-Error "Required runtime component '$dir' not found."
    }
}

# ── Validate Runtime ──────────────────────────────────────────────────────────
Write-Host ""
Write-Host "Validating runtime files..."

$kernelExe   = Join-Path $RuntimeDest "kernel\SiYuan-Kernel.exe"
$stageDir    = Join-Path $RuntimeDest "stage"
$appearDir   = Join-Path $RuntimeDest "appearance"
$guideDir    = Join-Path $RuntimeDest "guide"

if (-not (Test-Path $kernelExe)) {
    Write-Error "Runtime FAIL: kernel/SiYuan-Kernel.exe not found"
}
$kernelSizeMB = [math]::Round((Get-Item $kernelExe).Length / 1MB, 1)
if ($kernelSizeMB -lt 1) {
    Write-Error "Runtime FAIL: kernel is too small ($kernelSizeMB MB)"
}
Write-Host ("  {0,-14} OK  ({1} MB)" -f "kernel", $kernelSizeMB) -ForegroundColor Green

foreach ($dir in @($stageDir, $appearDir, $guideDir)) {
    $name = Split-Path $dir -Leaf
    if (-not (Test-Path $dir)) {
        Write-Error "Runtime FAIL: $name/ directory missing"
    }
    $files = Get-ChildItem $dir -Recurse -File -ErrorAction SilentlyContinue
    if (-not $files -or $files.Count -eq 0) {
        Write-Error "Runtime FAIL: $name/ is empty"
    }
    Write-Host ("  {0,-14} OK  ({1} files)" -f $name, $files.Count) -ForegroundColor Green
}

# ── Smoke Test: --help ────────────────────────────────────────────────────────
Write-Host ""
Write-Host "Running kernel smoke tests..."

Write-Host "  Testing: SiYuan-Kernel.exe --help"
try {
    $helpOutput = & $kernelExe "--help" 2>&1
    if ($LASTEXITCODE -eq 0 -or ($helpOutput -join "") -match "SiYuan|serve|Usage") {
        Write-Host "  --help            OK" -ForegroundColor Green
    } else {
        $preview = ($helpOutput | Select-Object -First 5) -join [Environment]::NewLine
        throw "SiYuan-Kernel.exe --help failed or returned unexpected output:`n$preview"
    }
} catch {
    throw "SiYuan-Kernel.exe --help smoke test failed: $_"
}

# ── Smoke Test: serve --help ──────────────────────────────────────────────────
Write-Host "  Testing: SiYuan-Kernel.exe serve --help"
try {
    $serveHelp = & $kernelExe "serve" "--help" 2>&1
    if ($LASTEXITCODE -eq 0 -or ($serveHelp -join "") -match "serve|port|workspace") {
        Write-Host "  serve --help      OK" -ForegroundColor Green
    } else {
        $preview = ($serveHelp | Select-Object -First 8) -join [Environment]::NewLine
        throw "SiYuan-Kernel.exe serve --help returned unexpected output:`n$preview"
    }
} catch {
    throw "SiYuan-Kernel.exe serve --help smoke test failed: $_"
}

# ── Smoke Test: Temporary Boot ────────────────────────────────────────────────
Write-Host ""
Write-Host "Running temporary kernel boot test..."

$tmpWorkspace = Join-Path $env:TEMP "aiks-siyuan-test-$([System.Guid]::NewGuid().ToString('N').Substring(0,8))"
New-Item -ItemType Directory -Force -Path $tmpWorkspace | Out-Null

# Find a free port
$testPort = $null
for ($p = 16800; $p -le 16899; $p++) {
    try {
        $tcp = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Loopback, $p)
        $tcp.Start()
        $tcp.Stop()
        $testPort = $p
        break
    } catch { }
}

if (-not $testPort) {
    Remove-Item $tmpWorkspace -Recurse -Force -ErrorAction SilentlyContinue
    throw "Could not find a free local port in range 16800-16899 for SiYuan boot test."
}

Write-Host "  Test port: $testPort"
Write-Host "  Test workspace: $tmpWorkspace"

$kernelArgs = @(
    "serve",
    "--workspace=$tmpWorkspace",
    "--wd=$RuntimeDest",
    "--port=$testPort",
    "--lang=zh-CN",
    "--mode=prod"
)

Write-Host "  Starting: SiYuan-Kernel.exe $($kernelArgs -join ' ')"

$proc = $null
try {
    $proc = Start-Process -FilePath $kernelExe `
        -ArgumentList $kernelArgs `
        -PassThru -NoNewWindow `
        -RedirectStandardOutput "$tmpWorkspace\stdout.log" `
        -RedirectStandardError "$tmpWorkspace\stderr.log"

    $apiUrl  = "http://127.0.0.1:$testPort/api/system/version"
    $ready   = $false
    $retryMs = @(250, 500, 500, 1000, 1000, 2000, 2000, 3000, 3000, 5000)

    foreach ($waitMs in $retryMs) {
        Start-Sleep -Milliseconds $waitMs

        if ($proc.HasExited) {
            $stderrPreview = ""
            if (Test-Path "$tmpWorkspace\stderr.log") {
                $stderrPreview = (Get-Content "$tmpWorkspace\stderr.log" | Select-Object -First 20) -join [Environment]::NewLine
            }
            throw "Kernel process exited early with code $($proc.ExitCode).`n$stderrPreview"
        }

        try {
            $resp = Invoke-RestMethod -Uri $apiUrl -Method Get -TimeoutSec 2 -ErrorAction Stop
            if ($resp.code -eq 0 -or $null -ne $resp.data) {
                $ready = $true
                break
            }
        } catch { }
    }

    if (-not $ready) {
        $stderrPreview = ""
        if (Test-Path "$tmpWorkspace\stderr.log") {
            $stderrPreview = (Get-Content "$tmpWorkspace\stderr.log" | Select-Object -First 20) -join [Environment]::NewLine
        }
        throw "Kernel did not become ready at $apiUrl within timeout.`n$stderrPreview"
    }

    $versionResp = Invoke-RestMethod -Uri $apiUrl -Method Get -TimeoutSec 5 -ErrorAction Stop
    $actualVersion = [string]$versionResp.data

    if ($actualVersion -ne $SiyuanVersion) {
        throw "Bundled SiYuan runtime version mismatch. Expected $SiyuanVersion, actual $actualVersion."
    }

    Write-Host "  Kernel boot      OK" -ForegroundColor Green
    Write-Host "  HTTP API         OK" -ForegroundColor Green
    Write-Host "  Version          $actualVersion" -ForegroundColor Green
}
finally {
    if ($proc -and -not $proc.HasExited) {
        try {
            $null = Invoke-RestMethod -Uri "http://127.0.0.1:$testPort/api/system/exit" `
                -Method Post -Body '{"force":false}' `
                -ContentType "application/json" `
                -TimeoutSec 5 -ErrorAction SilentlyContinue
        } catch { }

        try {
            $exited = $proc.WaitForExit(8000)
            if (-not $exited -and -not $proc.HasExited) {
                $proc.Kill()
                $proc.WaitForExit()
            }
        } catch {
            try { $proc.Kill() } catch { }
        }
    }

    Remove-Item $tmpWorkspace -Recurse -Force -ErrorAction SilentlyContinue
}

# Write version marker only after all validation succeeds.
Set-Content -LiteralPath $RuntimeVersionMarker -Value $SiyuanVersion -Encoding ASCII

# ── Clean staging ─────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "Cleaning staging directory..."
Remove-Item $StagingDir -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "  Staging cleaned."

# ── Final Summary ─────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "=== Setup Complete ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Version   :  $SiyuanVersion"
Write-Host "  Platform  :  $($cfg['platform'])"
Write-Host "  Installer :  SHA256 verified"
Write-Host ""
Write-Host "  Runtime:"
Write-Host ("    {0,-12} OK" -f "Kernel")
Write-Host ("    {0,-12} OK" -f "Stage")
Write-Host ("    {0,-12} OK" -f "Appearance")
Write-Host ("    {0,-12} OK" -f "Guide")
Write-Host ""
Write-Host "  Runtime root: $RuntimeDest" -ForegroundColor DarkGray
Write-Host ""
Write-Host "Embedded SiYuan runtime is ready." -ForegroundColor Green
Write-Host ""
Write-Host "Next steps:"
Write-Host "  Dev:     powershell -File scripts\dev.ps1"
Write-Host "  Release: powershell -File scripts\build-release.ps1"
