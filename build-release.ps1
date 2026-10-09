<#
.SYNOPSIS
    Builds all components in release mode, packages them into 'dist', and generates the Windows installer.
#>

$ErrorActionPreference = "Stop"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  Building HID Keyboard Layout Remapper (Release & Setup)" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$WorkspaceRoot = $PSScriptRoot
$DistDir = Join-Path $WorkspaceRoot "dist"

Write-Host "[1/4] Compiling Rust binaries with cargo build --release..." -ForegroundColor Green
cargo build --release --workspace

if ($LASTEXITCODE -ne 0) {
    Write-Host "Cargo build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "[2/4] Preparing distribution files in '$DistDir'..." -ForegroundColor Green
if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
}

$ReleaseBinDir = Join-Path $WorkspaceRoot "target\release"

$Binaries = @(
    "hid-remap.exe",
    "hid-remap-service.exe",
    "hid-remap-agent.exe",
    "hid-remap-gui.exe",
    "hid-remap-cli.exe"
)

foreach ($bin in $Binaries) {
    $src = Join-Path $ReleaseBinDir $bin
    if (Test-Path $src) {
        try {
            Copy-Item -Path $src -Destination $DistDir -Force -ErrorAction Stop
            Write-Host "  Copied $bin" -ForegroundColor Gray
        } catch {
            Write-Host "  Note: $bin is currently in use/locked. Using target/release/$bin for installer." -ForegroundColor Yellow
        }
    } else {
        Write-Host "  Warning: $bin not found in target/release" -ForegroundColor Yellow
    }
}

Copy-Item -Path (Join-Path $WorkspaceRoot "config.toml") -Destination $DistDir -Force
$IconSrc = Join-Path $WorkspaceRoot "crates\hid-remap-app\ui\assets\app_icon.ico"
if (Test-Path $IconSrc) {
    Copy-Item -Path $IconSrc -Destination (Join-Path $DistDir "app_icon.ico") -Force
}

# Copy driver package files
$DriverDistDir = Join-Path $DistDir "driver"
if (-not (Test-Path $DriverDistDir)) {
    New-Item -ItemType Directory -Path $DriverDistDir -Force | Out-Null
}
Copy-Item -Path (Join-Path $WorkspaceRoot "driver\*") -Destination $DriverDistDir -Recurse -Force

Write-Host "[3/4] Release files assembled in: $DistDir" -ForegroundColor Green

# 4. Build Installer
Write-Host "[4/4] Building Windows setup installer..." -ForegroundColor Green
$BuildInstallerScript = Join-Path $WorkspaceRoot "installer\build-installer.ps1"
if (Test-Path $BuildInstallerScript) {
    & powershell -ExecutionPolicy Bypass -File $BuildInstallerScript
}

Write-Host ""
Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  Build & Packaging Complete!" -ForegroundColor Cyan
Write-Host "  Installer: $DistDir\hid-remap-setup.exe" -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Cyan
