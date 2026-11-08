#Requires -Version 7.0
[CmdletBinding()]
param(
    [ValidatePattern('^\d{2}\.\d{1,2}$')]
    [string] $Patch
)

$ErrorActionPreference = 'Stop'
$folder = if ($Patch) { $Patch.Replace('.', '-') } else { 'latest' }
$url = "https://arammayhem.com/data/v1/$folder/augments.json"
$export = Invoke-RestMethod -Uri $url -TimeoutSec 30
$sourcePatch = $export.meta.patch
$statsDate = $export.meta.statsDate

if ($export.schemaVersion -ne 1 -or $sourcePatch -notmatch '^\d{2}\.\d{1,2}$' -or
    ($Patch -and $Patch -ne $sourcePatch) -or $statsDate -notmatch '^\d{4}-\d{2}-\d{2}$') {
    throw "Unexpected export metadata from $url"
}

$unrated = 0
$augments = @(
    foreach ($row in ($export.rows | Sort-Object -Property augmentId)) {
        if ($row.retiredPatch) { continue }
        if ($null -eq $row.pickRate) {
            $unrated++
            continue
        }
        if ([string]::IsNullOrWhiteSpace($row.name.en) -or
            $row.rarity -cnotin @('silver', 'gold', 'prismatic') -or
            ($row.pickRate -isnot [double] -and $row.pickRate -isnot [long] -and
             $row.pickRate -isnot [int]) -or $row.pickRate -lt 0 -or $row.pickRate -gt 100) {
            throw "Invalid augment row in $url ($($row.augmentId))"
        }
        [ordered]@{ name = $row.name.en; rarity = $row.rarity; pick_rate = $row.pickRate }
    }
)

if (@($augments.rarity | Sort-Object -Unique).Count -ne 3) {
    throw "Export is missing at least one augment rarity: $url"
}
if ($unrated) { Write-Warning "Skipped $unrated active augment(s) with no pick rate." }

$rows = $augments | ForEach-Object { '    ' + (ConvertTo-Json -InputObject $_ -Compress) }
$json = @"
{
  "patch": "$sourcePatch",
  "stats_date": "$statsDate",
  "augments": [
$($rows -join ",`n")
  ]
}
"@
$path = Join-Path $PSScriptRoot '../data/augments.json'
$temp = "$path.$([guid]::NewGuid()).tmp"
try {
    [IO.File]::WriteAllText($temp, ($json -replace '\r\n', "`n") + "`n")
    [IO.File]::Move($temp, $path, $true)
} finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp }
}

Write-Output "Patch $sourcePatch ($statsDate): wrote $($augments.Count) augments to data/augments.json"
