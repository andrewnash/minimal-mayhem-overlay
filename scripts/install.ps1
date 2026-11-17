<#
.SYNOPSIS
Installs Minimal Mayhem Overlay for the current user: copies the built exe to
%LOCALAPPDATA%\Programs, adds Desktop and Start menu shortcuts, turns on Start with Windows,
and starts it. No admin rights needed. Run with -Uninstall to remove all of it.
#>
param(
    [string] $Source = (Join-Path $PSScriptRoot '../target/release/minimal-mayhem-overlay.exe'),
    [switch] $NoStartup,
    [switch] $Uninstall
)
$ErrorActionPreference = 'Stop'

$name = 'Minimal Mayhem Overlay'
$dir = Join-Path $env:LOCALAPPDATA "Programs\$name"
$exe = Join-Path $dir 'minimal-mayhem-overlay.exe'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$runValue = 'MinimalMayhemOverlay'
$shortcuts = @(
    (Join-Path ([Environment]::GetFolderPath('Desktop')) "$name.lnk"),
    (Join-Path ([Environment]::GetFolderPath('Programs')) "$name.lnk")
)

# Ask a running copy to quit through its tray window so its icon is removed cleanly.
Add-Type -Namespace MayhemInstall -Name Native -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern IntPtr FindWindow(string className, IntPtr windowName);
[DllImport("user32.dll")]
public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
'@
$tray = [MayhemInstall.Native]::FindWindow('MinimalMayhemTray', [IntPtr]::Zero)
if ($tray -ne [IntPtr]::Zero) {
    [MayhemInstall.Native]::PostMessage($tray, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    $running = Get-Process minimal-mayhem-overlay -ErrorAction SilentlyContinue
    $running | Wait-Process -Timeout 5 -ErrorAction SilentlyContinue
    $running | Where-Object { -not $_.HasExited } | Stop-Process -Force
}

if ($Uninstall) {
    Remove-ItemProperty -Path $runKey -Name $runValue -ErrorAction SilentlyContinue
    $shortcuts | Where-Object { Test-Path $_ } | Remove-Item
    if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    Write-Host "Removed $name. Logs and samples stay in $env:LOCALAPPDATA\MinimalMayhemOverlay."
    return
}

$Source = (Resolve-Path $Source).Path
New-Item -ItemType Directory -Force $dir | Out-Null
Copy-Item $Source $exe -Force
Copy-Item (Join-Path $PSScriptRoot '../assets/icon.ico') (Join-Path $dir 'icon.ico') -Force
$picks = Join-Path (Split-Path $Source) 'champion-picks.json'
if (Test-Path $picks) { Copy-Item $picks $dir -Force }

$shell = New-Object -ComObject WScript.Shell
foreach ($path in $shortcuts) {
    $link = $shell.CreateShortcut($path)
    $link.TargetPath = $exe
    $link.WorkingDirectory = $dir
    $link.IconLocation = (Join-Path $dir 'icon.ico')
    $link.Description = 'ARAM Mayhem augment tiers and builds, shown over League'
    $link.Save()
}

if ($NoStartup) {
    Remove-ItemProperty -Path $runKey -Name $runValue -ErrorAction SilentlyContinue
} else {
    Set-ItemProperty -Path $runKey -Name $runValue -Value "`"$exe`" --startup"
}

Start-Process $exe -WorkingDirectory $dir
Write-Host "Installed to $dir"
Write-Host "Desktop and Start menu shortcuts added. Start with Windows: $(-not $NoStartup)"
