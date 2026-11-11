# Builds TidyTray and packages it as dist\TidyTray-v<version>-x64-Portable.zip
# (the same shape the release workflow publishes; the NSIS installer, when
# built, is copied next to the zip as TidyTray-v<version>-x64-Setup.exe)
# Usage: powershell -ExecutionPolicy Bypass -File scripts\build.ps1 [-NoInstaller] [-Test]
param(
    [switch]$NoInstaller,
    [switch]$Test
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$version = (Get-Content (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
$release = "TidyTray-v$version-x64-Portable.zip"
$dist = Join-Path $root 'dist'

Push-Location (Join-Path $root 'src-tauri')
try {
    if ($Test) {
        Write-Host '==> cargo test' -ForegroundColor Cyan
        cargo test --quiet
        if ($LASTEXITCODE -ne 0) { throw 'tests failed' }
    }
    $hasTauriCli = $null -ne (cargo --list | Select-String -SimpleMatch 'tauri')
    if ($hasTauriCli -and -not $NoInstaller) {
        Write-Host '==> cargo tauri build (exe + NSIS installer)' -ForegroundColor Cyan
        cargo tauri build
    } else {
        Write-Host '==> cargo build --release (portable exe only)' -ForegroundColor Cyan
        cargo build --release
    }
    if ($LASTEXITCODE -ne 0) { throw 'build failed' }
} finally {
    Pop-Location
}

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'src-tauri\target' }
$exe = Get-ChildItem -Path (Join-Path $targetDir 'release') -Filter '*.exe' |
    Where-Object { $_.Name -in @('TidyTray.exe', 'tidytray.exe') } | Select-Object -First 1
if (-not $exe) { throw "TidyTray.exe not found in $targetDir\release" }

$stage = Join-Path $dist 'stage'
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $exe.FullName (Join-Path $stage 'TidyTray.exe')
Copy-Item (Join-Path $root 'LICENSE') $stage
$installer = Get-ChildItem -Path (Join-Path $targetDir 'release\bundle\nsis') -Filter '*.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($installer) { Copy-Item $installer.FullName (Join-Path $dist "TidyTray-v$version-x64-Setup.exe") }

$zip = Join-Path $dist $release
Remove-Item -Force $zip -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
Remove-Item -Recurse -Force $stage
$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
Write-Host "==> $zip ($size MB)" -ForegroundColor Green
