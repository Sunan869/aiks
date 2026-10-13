# Windows PowerShell 5.1+ -- read-only acceptance preparation.
# Run: powershell -ExecutionPolicy Bypass -File scripts\verify-offline-backup-readiness.ps1
param(
    [string]$TestRoot = "E:\AIKS-Test\data",
    [string]$ExpectedBranch = "feature/offline-knowledge-evolution-20261009"
)
$ErrorActionPreference = "Stop"
$repo = Split-Path $PSScriptRoot -Parent
$root = [IO.Path]::GetFullPath($TestRoot).TrimEnd('\', '/')
function Assert-Condition([bool]$ok, [string]$message) {
    if (-not $ok) { throw $message }
}
function Within([string]$candidate, [string]$container) {
    $candidate = [IO.Path]::GetFullPath($candidate).TrimEnd('\', '/')
    $container = [IO.Path]::GetFullPath($container).TrimEnd('\', '/')
    return $candidate.Equals($container, [StringComparison]::OrdinalIgnoreCase) -or
        $candidate.StartsWith($container + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)
}
$head = (& git -C $repo rev-parse --abbrev-ref HEAD 2>$null | Out-String).Trim()
Assert-Condition ($LASTEXITCODE -eq 0) "Git checkout not found; run this script from the AIKS repository."
Assert-Condition ($head -eq $ExpectedBranch) "Wrong branch: $head"
$project = [IO.Path]::GetFullPath($repo).TrimEnd('\', '/')
Assert-Condition (-not (Within $root $project)) "Testing data may not be inside the source checkout."
$homeDir = [Environment]::GetFolderPath('UserProfile')
Assert-Condition (-not (Within $root $homeDir)) "Use a dedicated test directory outside your user profile."
$active = $env:AIKS_DATA_DIR
if ($active) {
    $actual = [IO.Path]::GetFullPath($active).TrimEnd('\', '/')
    Assert-Condition ($actual.Equals($root, [StringComparison]::OrdinalIgnoreCase)) "AIKS_DATA_DIR is not the isolated test root."
} else {
    Write-Warning "AIKS_DATA_DIR is unset. Set it in the SAME PowerShell session before starting scripts/dev.ps1."
}
Assert-Condition (-not $env:SIYUAN_URL) "SIYUAN_URL override must be cleared for managed-kernel testing."
Assert-Condition (-not $env:AIKS_BACKEND_MODE -or $env:AIKS_BACKEND_MODE -ne 'service_remote') "Remote backend cannot be used for offline acceptance."
Assert-Condition (Test-Path -LiteralPath (Join-Path $repo "scripts\dev.ps1")) "Missing developer launch script."
Assert-Condition (Test-Path -LiteralPath (Join-Path $repo "apps\aiks-desktop\src-tauri\src\backup_commands.rs")) "Backup command source missing."
$state = Join-Path $root "aiks.db"
$workspace = Join-Path $root "siyuan\workspace"
$hasState = Test-Path -LiteralPath $state
$hasWorkspace = Test-Path -LiteralPath $workspace
if ($hasState -or $hasWorkspace) {
    Write-Warning "Test root already contains AIKS state. Script will NOT delete, overwrite or reset it."
}
Write-Host "SAFE TO PREPARE WINDOWS SMOKE TEST" -ForegroundColor Green
Write-Host "Branch: $head"
Write-Host "Test root: $root"
Write-Host "Existing database: $hasState; workspace: $hasWorkspace"
Write-Host 'Next: set $env:AIKS_DATA_DIR to the test root in this PowerShell session, then run .\scripts\dev.ps1.'
Write-Host "In the Desktop Settings, choose a NEW backup output directory outside the data root; verify it, and restore to another NEW directory."
Write-Host "If SiYuan stop/restart or recovery fails, do not reuse personal data; preserve logs and report the error."
