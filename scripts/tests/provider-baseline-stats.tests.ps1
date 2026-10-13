#requires -Version 5.1
$ErrorActionPreference = "Stop"
. (Join-Path (Split-Path -Parent $PSScriptRoot) "provider-baseline-stats.ps1")
function Assert-Equal($Actual, $Expected, [string]$Message) {
    if ($Actual -ne $Expected) { throw "$Message : expected $Expected got $Actual" }
}
$records = @(
    [PSCustomObject]@{ iteration = 1; elapsed_ms = 900; success = $true },
    [PSCustomObject]@{ iteration = 2; elapsed_ms = 80; success = $true },
    [PSCustomObject]@{ iteration = 3; elapsed_ms = 40; success = $true },
    [PSCustomObject]@{ iteration = 4; elapsed_ms = 60; success = $true }
)
$summary = Get-AiksProviderTimingSummary -Records $records
Assert-Equal $summary.cold_scan_ms 900 "cold"
Assert-Equal $summary.warm_p50_ms 60 "nearest rank p50"
Assert-Equal $summary.warm_p95_ms 80 "nearest rank p95"
Assert-Equal $summary.warm_max_ms 80 "maximum"
Assert-Equal $summary.warm_scan_count 3 "warm count"
Assert-Equal $summary.cold_to_warm_p50_ratio 15 "cold warm ratio"
$single = Get-AiksProviderTimingSummary -Records @($records[0])
Assert-Equal $single.warm_scan_count 0 "no warm sample"
if ($null -ne $single.warm_p50_ms) { throw "Single sample must not invent warm percentile" }
$failure = Get-AiksProviderTimingSummary -Records @(
    [PSCustomObject]@{ iteration = 1; elapsed_ms = 7; success = $false }
)
Assert-Equal $failure.successful_runs 0 "failed scans excluded"
if ($null -ne $failure.cold_scan_ms) { throw "Failed scan must not have a cold baseline" }
Write-Host "Provider baseline statistics contract passed"
