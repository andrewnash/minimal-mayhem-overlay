#Requires -Version 7.0
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$snapshot = Get-Content (Join-Path $PSScriptRoot '../data/augments.json') -Raw | ConvertFrom-Json
$major, $minor = $snapshot.patch.Split('.')
$cdn = "https://ddragon.leagueoflegends.com/cdn/$([int]$major - 10).$minor.1"
$catalog = Invoke-RestMethod 'https://arammayhem.com/data/v1/latest/items.json' -TimeoutSec 30
$dragon = Invoke-RestMethod "$cdn/data/en_US/item.json" -TimeoutSec 30
$client = [Net.Http.HttpClient]::new()
$client.Timeout = [TimeSpan]::FromSeconds(30)
$sprites = @{}
$atlas = [IO.MemoryStream]::new()
$size = 20
$count = 0
try {
    foreach ($item in ($catalog.rows | Sort-Object { $_.name.en })) {
        if (-not $item.gameItemId -or -not $item.name.en) { continue }
        $image = $dragon.data.PSObject.Properties[[string]$item.gameItemId].Value.image
        if (-not $image.sprite) { continue }
        if (-not $sprites.ContainsKey($image.sprite)) {
            $bytes = $client.GetByteArrayAsync("$cdn/img/sprite/$($image.sprite)").GetAwaiter().GetResult()
            $stream = [IO.MemoryStream]::new($bytes)
            try {
                $sheet = [Drawing.Image]::FromStream($stream)
                try {
                    $sprites[$image.sprite] = [Drawing.Bitmap]::new($sheet)
                } finally { $sheet.Dispose() }
            } finally { $stream.Dispose() }
        }

        $name = [Text.Encoding]::UTF8.GetBytes($item.name.en)
        if ($name.Length -gt 255) { throw "Item name too long: $($item.name.en)" }
        $atlas.WriteByte([byte]$name.Length)
        $atlas.Write($name, 0, $name.Length)
        $bitmap = [Drawing.Bitmap]::new($size, $size, [Drawing.Imaging.PixelFormat]::Format32bppRgb)
        try {
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.DrawImage($sprites[$image.sprite], [Drawing.Rectangle]::new(0, 0, $size, $size),
                    $image.x, $image.y, $image.w, $image.h, [Drawing.GraphicsUnit]::Pixel)
            } finally { $graphics.Dispose() }
            $pixels = $bitmap.LockBits([Drawing.Rectangle]::new(0, 0, $size, $size),
                [Drawing.Imaging.ImageLockMode]::ReadOnly, [Drawing.Imaging.PixelFormat]::Format32bppRgb)
            try {
                $row = [byte[]]::new($size * 4)
                for ($y = 0; $y -lt $size; $y++) {
                    [Runtime.InteropServices.Marshal]::Copy([IntPtr]::Add($pixels.Scan0, $y * $pixels.Stride), $row, 0, $row.Length)
                    $atlas.Write($row, 0, $row.Length)
                }
            } finally { $bitmap.UnlockBits($pixels) }
        } finally { $bitmap.Dispose() }
        $count++
    }
    if (-not $count) { throw 'No item icons were generated.' }
    $path = Join-Path $PSScriptRoot '../data/item-icons.bin'
    $temp = "$path.$([guid]::NewGuid()).tmp"
    try {
        [IO.File]::WriteAllBytes($temp, $atlas.ToArray())
        [IO.File]::Move($temp, $path, $true)
    } finally {
        if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp }
    }
    Write-Output "Patch $($snapshot.patch): packed $count Data Dragon icons."
} finally {
    foreach ($sprite in $sprites.Values) { $sprite.Dispose() }
    $atlas.Dispose()
    $client.Dispose()
}
