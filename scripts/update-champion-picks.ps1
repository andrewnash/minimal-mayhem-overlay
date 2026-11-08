#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^[A-Za-z0-9]+$')]
    [string] $Champion,
    [ValidatePattern('^\d{2}\.\d{1,2}$')]
    [string] $SourcePatch,
    [string] $OutputPath = (Join-Path $PSScriptRoot '../target/release/champion-picks.json')
)

$ErrorActionPreference = 'Stop'
$snapshot = Get-Content (Join-Path $PSScriptRoot '../data/augments.json') -Raw | ConvertFrom-Json
$major, $minor = $snapshot.patch.Split('.')
if (-not $SourcePatch) { $SourcePatch = "$([int]$major - 10).$minor" }
if ($SourcePatch.Split('.')[1] -ne $minor) { throw 'The AramHex patch must match the bundled patch.' }

$export = "https://arammayhem.com/data/v1/$($snapshot.patch.Replace('.', '-'))"
$aramHex = "https://raw.githubusercontent.com/czgreat/aramhex-data/main/patches/$SourcePatch/normal"
$champions = Invoke-RestMethod "$export/champions.json" -TimeoutSec 30
$championRow = $champions.rows | Where-Object championId -EQ $Champion | Select-Object -First 1
if (-not $championRow.name.'zh-CN') { throw "Unknown champion: $Champion" }

$meta = Invoke-RestMethod "$aramHex/meta.json" -TimeoutSec 30
if ($meta.patch -ne $SourcePatch) { throw 'Unexpected AramHex patch.' }
$heroes = Invoke-RestMethod "$aramHex/heroes.json" -TimeoutSec 30
$hero = @($heroes.PSObject.Properties | Where-Object { $_.Value.name -ceq $championRow.name.'zh-CN' })
if ($hero.Count -ne 1 -or $hero[0].Value.games -le 0) { throw "No unique match sample for $Champion" }

$augments = Invoke-RestMethod "$export/augments.json" -TimeoutSec 30
$names = Invoke-RestMethod "$aramHex/augments.json" -TimeoutSec 30
$allPicks = Invoke-RestMethod "$aramHex/hero_augments.json" -TimeoutSec 60
$heroPicks = $allPicks.PSObject.Properties[$hero[0].Name].Value
$active = [System.Collections.Generic.HashSet[string]]::new([string[]]$snapshot.augments.id)
$nameGroups = $names.PSObject.Properties | Group-Object { "$($_.Value.rarity)|$($_.Value.name)" } -AsHashTable -AsString
$picks = [ordered]@{}
foreach ($row in ($augments.rows | Sort-Object -Property augmentId)) {
    if (-not $active.Contains($row.augmentId) -or -not $row.name.'zh-CN') { continue }
    $matchesByName = @($nameGroups["$($row.rarity)|$($row.name.'zh-CN')"])
    if ($matchesByName.Count -ne 1 -or -not $matchesByName[0]) { continue }
    $count = $heroPicks.PSObject.Properties[$matchesByName[0].Name].Value.games
    if ($count -gt 0 -and $count -le $hero[0].Value.games) { $picks[$row.augmentId] = $count }
}
if (-not $picks.Count) { throw "No matching augment samples for $Champion" }

$weaponItems = @()
if ($active.Contains('weapon_of_destiny')) {
    try {
        $itemStats = Invoke-RestMethod "$aramHex/hero_augment_items.json" -TimeoutSec 30
    } catch {
        if ($_.Exception.Response.StatusCode -ne [Net.HttpStatusCode]::NotFound) { throw }
    }
    $records = $itemStats.PSObject.Properties[$hero[0].Name].Value.'3081'.items
    if ($records) {
        $catalog = Invoke-RestMethod 'https://arammayhem.com/data/v1/latest/items.json' -TimeoutSec 30
        $itemNames = @{}
        foreach ($item in $catalog.rows) {
            if ($item.gameItemId -and $item.name.en) { $itemNames[[string]$item.gameItemId] = $item.name.en }
        }
        $weaponItems = @($records |
            Where-Object { $_.games -ge 1000 -and -not $_.low_sample -and $itemNames.ContainsKey([string]$_.item) } |
            Sort-Object -Property @{Expression = 'games'; Descending = $true}, item |
            ForEach-Object { $itemNames[[string]$_.item] } |
            Select-Object -Unique -First 2)
    }
}

$output = [ordered]@{
    patch = $snapshot.patch
    source_patch = $SourcePatch
    champion = $championRow.championId.ToLowerInvariant()
    games = $hero[0].Value.games
    picks = $picks
    weapon_items = $weaponItems
    attribution = 'ARAM Mayhem statistics (https://github.com/czgreat/aramhex-data), CC BY-NC 4.0 (https://creativecommons.org/licenses/by-nc/4.0/)'
}
$path = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputPath)
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path)) | Out-Null
$temp = "$path.$([guid]::NewGuid()).tmp"
try {
    [IO.File]::WriteAllText($temp, (ConvertTo-Json -InputObject $output -Depth 4 -Compress) + "`n")
    [IO.File]::Move($temp, $path, $true)
} finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp }
}
Write-Output "$($championRow.championId), AramHex ${SourcePatch}: $($picks.Count) matched augment counts across $($output.games) champion games."
if ($weaponItems.Count) { Write-Output "Weapon of Destiny item records: $($weaponItems -join ' / ')" }
