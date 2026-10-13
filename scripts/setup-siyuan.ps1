param(
    [switch]$Force,
    [string]$RuntimeArchive
)

$ErrorActionPreference = "Stop"
$ScriptDir = $PSScriptRoot
$Fetcher = Join-Path $ScriptDir "fetch-siyuan-runtime.py"

if (-not (Test-Path -LiteralPath $Fetcher)) {
    throw "Runtime fetcher not found: $Fetcher"
}

$Python = Get-Command python -ErrorAction SilentlyContinue
if (-not $Python) {
    $Python = Get-Command python3 -ErrorAction SilentlyContinue
}
if (-not $Python) {
    throw "Python 3 is required to prepare the cross-platform SiYuan runtime."
}

$args = @($Fetcher)
if ($Force) {
    $args += "--force"
}
if (-not [string]::IsNullOrWhiteSpace($RuntimeArchive)) {
    $args += @("--runtime-archive", $RuntimeArchive)
}

& $Python.Source @args
if ($LASTEXITCODE -ne 0) {
    throw "SiYuan runtime setup failed with exit code $LASTEXITCODE"
}
