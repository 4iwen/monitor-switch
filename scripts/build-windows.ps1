$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$Dist = Join-Path $Root "dist\windows"

Push-Location $Root
try {
    cargo build --release --bin monitor-switch --bin monitor-switch-app

    New-Item -ItemType Directory -Force -Path $Dist | Out-Null
    Copy-Item "target\release\monitor-switch.exe" $Dist -Force
    Copy-Item "target\release\monitor-switch-app.exe" $Dist -Force
    Copy-Item "README.md" $Dist -Force
    Copy-Item "LICENSE" $Dist -Force

    $Archive = Join-Path $Root "dist\Monitor-Switch-Windows.zip"
    if (Test-Path $Archive) {
        Remove-Item $Archive
    }
    Compress-Archive -Path "$Dist\*" -DestinationPath $Archive
    Write-Output $Archive
}
finally {
    Pop-Location
}
