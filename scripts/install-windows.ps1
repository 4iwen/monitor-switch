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

    # Remove any Startup shortcut left by older installs; the app does not start at login.
    Remove-Item $ShortcutPath -Force -ErrorAction SilentlyContinue

    Write-Output "Monitor Switch installed to $InstallDir"
    Write-Output "Open it from the Start menu or run Monitor Switch.exe. It does not start at login."
}
finally {
    Pop-Location
}
