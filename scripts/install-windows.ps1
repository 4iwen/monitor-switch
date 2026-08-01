$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\Monitor Switch"
$AppPath = Join-Path $InstallDir "Monitor Switch.exe"
$StartupDir = [Environment]::GetFolderPath("Startup")
$ShortcutPath = Join-Path $StartupDir "Monitor Switch.lnk"

Push-Location $Root
try {
    cargo build --release --bin monitor-switch-app

    Get-Process "Monitor Switch", "monitor-switch-app" -ErrorAction SilentlyContinue | Stop-Process -Force
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item "target\release\monitor-switch-app.exe" $AppPath -Force
    Remove-Item (Join-Path $InstallDir "monitor-switch-app.exe") -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $InstallDir "monitor-switch.exe") -Force -ErrorAction SilentlyContinue
    Copy-Item "README.md" $InstallDir -Force
    Copy-Item "LICENSE" $InstallDir -Force

    $Shell = New-Object -ComObject WScript.Shell
    $Shortcut = $Shell.CreateShortcut($ShortcutPath)
    $Shortcut.TargetPath = $AppPath
    $Shortcut.WorkingDirectory = $InstallDir
    $Shortcut.Description = "Monitor Switch"
    $Shortcut.Save()

    Start-Process $AppPath
    Write-Output "Monitor Switch installed to $InstallDir"
    Write-Output "Startup shortcut created at $ShortcutPath"
}
finally {
    Pop-Location
}
