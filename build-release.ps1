<#
.SYNOPSIS
    Builds all components in release mode and packages them into the 'dist' directory.
#>

$ErrorActionPreference = "Stop"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  Building HID Keyboard Layout Remapper (Release)" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$WorkspaceRoot = $PSScriptRoot
$DistDir = Join-Path $WorkspaceRoot "dist"

Write-Host "[1/3] Compiling Rust binaries with cargo build --release..." -ForegroundColor Green
cargo build --release --workspace

if ($LASTEXITCODE -ne 0) {
    Write-Host "Cargo build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "[2/3] Preparing distribution package at '$DistDir'..." -ForegroundColor Green
if (Test-Path $DistDir) {
    Remove-Item -Path $DistDir -Recurse -Force
}
New-Item -ItemType Directory -Path $DistDir -Force | Out-Null

$ReleaseBinDir = Join-Path $WorkspaceRoot "target\release"

$Binaries = @(
    "hid-remap-service.exe",
    "hid-remap-agent.exe",
    "hid-remap-gui.exe",
    "hid-remap-cli.exe"
)

foreach ($bin in $Binaries) {
    $src = Join-Path $ReleaseBinDir $bin
    if (Test-Path $src) {
        Copy-Item -Path $src -Destination $DistDir -Force
        Write-Host "  Copied $bin" -ForegroundColor Gray
    } else {
        Write-Host "  Warning: $bin not found in target/release" -ForegroundColor Yellow
    }
}

Copy-Item -Path (Join-Path $WorkspaceRoot "config.toml") -Destination $DistDir -Force

# Copy driver package files
$DriverDistDir = Join-Path $DistDir "driver"
New-Item -ItemType Directory -Path $DriverDistDir -Force | Out-Null
Copy-Item -Path (Join-Path $WorkspaceRoot "driver\*") -Destination $DriverDistDir -Recurse -Force

Write-Host "[3/3] Release package assembled successfully!" -ForegroundColor Green
Write-Host "Distribution files are in: $DistDir" -ForegroundColor White
Write-Host "==========================================================" -ForegroundColor Cyan

