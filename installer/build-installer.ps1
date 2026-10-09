<#
.SYNOPSIS
    Builds the Inno Setup installer for HID Keyboard Layout Remapper.
#>

$ErrorActionPreference = "Stop"

$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$DistDir = Join-Path $WorkspaceRoot "dist"
$IssFile = Join-Path $PSScriptRoot "hid-remap.iss"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  Building HID Keyboard Layout Remapper Setup Installer" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 1. Locate ISCC.exe (Inno Setup Compiler)
$InnoCompilerCandidates = @(
    (Get-Command "ISCC.exe" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
    "C:\Users\sgt23\Documents\Projects\teams_fullscreen_ext_v0.1.0\.tools\inno\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
)

$IsccPath = $null
foreach ($path in $InnoCompilerCandidates) {
    if ($path -and (Test-Path $path)) {
        $IsccPath = $path
        break
    }
}

if (-not $IsccPath) {
    Write-Host "Error: Inno Setup Compiler (ISCC.exe) not found!" -ForegroundColor Red
    Write-Host "Please install Inno Setup 6 via: winget install JRSoftware.InnoSetup" -ForegroundColor Yellow
    exit 1
}

Write-Host "[1/3] Found Inno Setup Compiler at:" -ForegroundColor Green
Write-Host "      $IsccPath" -ForegroundColor Gray

# 2. Verify release executable exists
$ReleaseExe = Join-Path $WorkspaceRoot "target\release\hid-remap.exe"
if (-not (Test-Path $ReleaseExe)) {
    Write-Host "[2/3] target\release\hid-remap.exe not found. Building with cargo build --release..." -ForegroundColor Yellow
    Set-Location $WorkspaceRoot
    cargo build --release -p hid-remap-app
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Cargo build failed!" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "[2/3] Verified release binary: $ReleaseExe" -ForegroundColor Green
}

# Ensure dist directory exists
if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
}

# 3. Compile Inno Setup Script
Write-Host "[3/3] Compiling installer with ISCC..." -ForegroundColor Green
& $IsccPath $IssFile

if ($LASTEXITCODE -ne 0) {
    Write-Host "ISCC compilation failed!" -ForegroundColor Red
    exit 1
}

# Create a generic shortcut filename dist/hid-remap-setup.exe
$VersionedSetup = Join-Path $DistDir "hid-remap-setup-v0.1.0.exe"
$GenericSetup = Join-Path $DistDir "hid-remap-setup.exe"

if (Test-Path $VersionedSetup) {
    Copy-Item -Path $VersionedSetup -Destination $GenericSetup -Force
    $sizeMb = [math]::Round((Get-Item $VersionedSetup).Length / 1MB, 2)
    Write-Host ""
    Write-Host "==========================================================" -ForegroundColor Green
    Write-Host "  Installer generated successfully!" -ForegroundColor Green
    Write-Host "  Setup Executable: $VersionedSetup ($sizeMb MB)" -ForegroundColor White
    Write-Host "  Generic Copy:     $GenericSetup" -ForegroundColor White
    Write-Host "==========================================================" -ForegroundColor Green
}

