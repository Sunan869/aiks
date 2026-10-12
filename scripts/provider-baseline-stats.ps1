#requires -Version 5.1
# Pure statistics; input must contain sanitized timing records only.
function Get-AiksProviderTimingSummary {
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][object[]]$Records)

    $successful = @($Records | Where-Object { $_.success -eq $true })
    if ($successful.Count -eq 0) {
        return [PSCustomObject]@{
            successful_runs = 0
            cold_scan_ms = $null
            warm_scan_count = 0
            warm_p50_ms = $null
            warm_p95_ms = $null
            warm_max_ms = $null
            cold_to_warm_p50_ratio = $null
        }
    }

    # A successful run after a failed scan is not a comparable warm scan.
    $sorted = @($successful | Sort-Object { [int]$_.iteration })
    $cold = [long]$sorted[0].elapsed_ms
    $warm = @($sorted | Select-Object -Skip 1 | ForEach-Object { [long]$_.elapsed_ms } | Sort-Object)
    if ($warm.Count -eq 0) {
        return [PSCustomObject]@{
            successful_runs = $successful.Count
            cold_scan_ms = $cold
            warm_scan_count = 0
            warm_p50_ms = $null
            warm_p95_ms = $null
            warm_max_ms = $null
            cold_to_warm_p50_ratio = $null
        }
    }
    $p50Index = [Math]::Ceiling($warm.Count * 0.50) - 1
    $p95Index = [Math]::Ceiling($warm.Count * 0.95) - 1
    $p50 = [long]$warm[$p50Index]
    $p95 = [long]$warm[$p95Index]
    return [PSCustomObject]@{
        successful_runs = $successful.Count
        cold_scan_ms = $cold
        warm_scan_count = $warm.Count
        warm_p50_ms = $p50
        warm_p95_ms = $p95
        warm_max_ms = [long]$warm[-1]
        cold_to_warm_p50_ratio = if ($p50 -gt 0) { [Math]::Round($cold / $p50, 2) } else { $null }
    }
}
