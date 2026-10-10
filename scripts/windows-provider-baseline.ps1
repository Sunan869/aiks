#requires -Version 5.1
<#
Read-only Provider discovery timing in an isolated AIKS data directory.
Never changes upstream Provider files or the user's existing AIKS state.
Only aggregate timing/exit status is written; stdout, file paths and tokens are not exported.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ConfigPath,
    [ValidateRange(1, 20)]
    [int]$Runs = 3,
    [string]$ReportPath = "",
    [switch]$KeepSandbox
)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$resolvedConfig = (Resolve-Path -LiteralPath $ConfigPath -ErrorAction Stop).Path
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Rust Cargo is required for this read-only Provider acceptance script."
}
$exe = Join-Path $repoRoot "target\debug\aiks-cli.exe"
if (-not (Test-Path -LiteralPath $exe)) {
    & cargo build -p aiks-cli --manifest-path (Join-Path $repoRoot "Cargo.toml") | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to build aiks-cli." }
}
if (-not (Test-Path -LiteralPath $exe)) { throw "aiks-cli.exe is unavailable." }

if (-not $ReportPath) {
    $ReportPath = Join-Path (Get-Location).Path ("aiks-provider-baseline-" + (Get-Date -Format "yyyyMMdd-HHmmss") + ".json")
}
if (Test-Path -LiteralPath $ReportPath) { throw "Report already exists; refusing to overwrite: $ReportPath" }

$sandbox = Join-Path $env:TEMP ("aiks-provider-acceptance-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $sandbox -Force | Out-Null
$priorDataDir = [Environment]::GetEnvironmentVariable("AIKS_DATA_DIR", "Process")
$records = New-Object System.Collections.Generic.List[object]
$scanFailure = $null
try {
    # Isolates persistent scan bookkeeping; Provider source directories stay untouched.
    [Environment]::SetEnvironmentVariable("AIKS_DATA_DIR", $sandbox, "Process")
    for ($i = 1; $i -le $Runs; $i++) {
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $exit = -1
        try {
            # Redirect everything: source paths, tokens and raw transcripts must
            # never enter the aggregate acceptance report.
            & $exe --config $resolvedConfig scan *> $null
            $exit = $LASTEXITCODE
        }
        catch {
            # Native-process exceptions also yield a sanitized failed record.
            $exit = -1
        }
        finally {
            $watch.Stop()
        }
        $records.Add([PSCustomObject]@{
            iteration = $i
            elapsed_ms = $watch.ElapsedMilliseconds
            exit_code = $exit
            success = ($exit -eq 0)
        })
        if ($exit -ne 0) {
            $scanFailure = "Scan iteration $i failed (exit $exit); see local private logs for details."
            break
        }
    }
    $report = [PSCustomObject]@{
        schema_version = 1
        platform = "windows"
        measured_at_utc = [DateTime]::UtcNow.ToString("o")
        requested_runs = $Runs
        completed_runs = $records.Count
        all_success = ($null -eq $scanFailure)
        # No paths, configuration, stdout, stderr, tokens or source content.
        results = @($records.ToArray())
    }
    $report | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $ReportPath -Encoding UTF8 -NoNewline
    Write-Host ("Saved aggregate Provider timing report: " + $ReportPath)
    if ($null -ne $scanFailure) { throw $scanFailure }
}
finally {
    [Environment]::SetEnvironmentVariable("AIKS_DATA_DIR", $priorDataDir, "Process")
    if (-not $KeepSandbox -and (Test-Path -LiteralPath $sandbox)) {
        # Remove only the isolated temporary directory created above.
        Remove-Item -LiteralPath $sandbox -Force -Recurse
    }
}
