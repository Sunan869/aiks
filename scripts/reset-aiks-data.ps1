[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [switch]$Force
)

$ErrorActionPreference = "Stop"

function Get-NormalizedFullPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        throw "AIKS data root is empty."
    }

    return [System.IO.Path]::GetFullPath($Path.Trim())
}

function Assert-SafeDataRoot {
    param([Parameter(Mandatory = $true)][string]$Path)

    $pathRoot = [System.IO.Path]::GetPathRoot($Path)
    if ([string]::IsNullOrWhiteSpace($pathRoot)) {
        throw "Cannot determine the filesystem root for AIKS data path: $Path"
    }

    $dangerousPaths = @(
        $pathRoot,
        $env:USERPROFILE,
        $env:LOCALAPPDATA
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | ForEach-Object {
        Get-NormalizedFullPath $_
    }

    foreach ($dangerousPath in $dangerousPaths) {
        if ([string]::Equals($Path.TrimEnd('\', '/'), $dangerousPath.TrimEnd('\', '/'), [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to delete unsafe path: $Path"
        }
    }
}

$defaultRoot = Join-Path $env:LOCALAPPDATA "AIKnowledgeSync"
$pointerFile = Join-Path $defaultRoot "data-root.txt"

if (-not [string]::IsNullOrWhiteSpace($env:AIKS_DATA_DIR)) {
    $dataRootCandidate = $env:AIKS_DATA_DIR
    $dataRootSource = "AIKS_DATA_DIR"
}
elseif (Test-Path -LiteralPath $pointerFile -PathType Leaf) {
    $dataRootCandidate = (Get-Content -LiteralPath $pointerFile -Raw).Trim()
    $dataRootSource = $pointerFile
}
else {
    $dataRootCandidate = $defaultRoot
    $dataRootSource = "default"
}

$dataRoot = Get-NormalizedFullPath $dataRootCandidate
Assert-SafeDataRoot $dataRoot

Write-Host "=== AIKS Data Reset ===" -ForegroundColor Cyan
Write-Host "Resolved data root : $dataRoot"
Write-Host "Resolved from      : $dataRootSource"
Write-Host ""
Write-Host "This removes AIKS state, config, logs, archives, and the embedded SiYuan workspace." -ForegroundColor Yellow
Write-Host "Original Codex / Claude / Gemini / OpenCode source data is not touched." -ForegroundColor Yellow

if (-not (Test-Path -LiteralPath $dataRoot -PathType Container)) {
    Write-Host "Nothing to delete. The AIKS data root does not exist." -ForegroundColor Green
    exit 0
}

if (-not $Force) {
    $confirmation = Read-Host "Type RESET to continue"
    if ($confirmation -cne "RESET") {
        Write-Host "Cancelled."
        exit 0
    }
}

foreach ($processName in @("AIKS", "aiks-desktop", "SiYuan-Kernel")) {
    Get-Process -Name $processName -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
}

Start-Sleep -Milliseconds 300

if ($PSCmdlet.ShouldProcess($dataRoot, "Delete the complete AIKS data root")) {
    Remove-Item -LiteralPath $dataRoot -Recurse -Force
}

Write-Host "AIKS data reset complete." -ForegroundColor Green
Write-Host "Start .\scripts\dev.ps1 to initialize a fresh AIKS database and embedded SiYuan workspace."
