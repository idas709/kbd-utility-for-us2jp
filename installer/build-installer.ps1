<#
.SYNOPSIS
    Builds the Windows Installer (MSI) and Setup Executable (EXE) for HID Keyboard Layout Remapper.
    Features:
      - Windows Installer (.msi) with All Users / Current User selection dialog
      - Inno Setup (.exe) standalone wizard
#>

$ErrorActionPreference = "Stop"

$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$DistDir = Join-Path $WorkspaceRoot "dist"
$PackageWxs = Join-Path $PSScriptRoot "Package.wxs"
$PackageJaWxl = Join-Path $PSScriptRoot "Package.ja-JP.wxl"
$PackageEnWxl = Join-Path $PSScriptRoot "Package.en-US.wxl"
$IssFile = Join-Path $PSScriptRoot "hid-remap.iss"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  Building HID Keyboard Layout Remapper Installers" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# Ensure license RTF files exist
$LicenseGenScript = Join-Path $PSScriptRoot "gen_license.ps1"
if (-not (Test-Path (Join-Path $PSScriptRoot "license-ja.rtf"))) {
    if (Test-Path $LicenseGenScript) {
        & powershell -ExecutionPolicy Bypass -File $LicenseGenScript | Out-Null
    }
}

# 1. Verify release executable exists
$ReleaseExe = Join-Path $WorkspaceRoot "target\release\hid-remap.exe"
if (-not (Test-Path $ReleaseExe)) {
    Write-Host "[1/3] target\release\hid-remap.exe not found. Building with cargo build --release..." -ForegroundColor Yellow
    Set-Location $WorkspaceRoot
    cargo build --release -p hid-remap-app
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Cargo build failed!" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "[1/3] Verified release binary: $ReleaseExe" -ForegroundColor Green
}

# Ensure dist directory exists
if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
}

# 2. Build Windows Installer (.msi) using WiX Toolset
Write-Host "[2/3] Building Windows Installer (.msi) with WiX Toolset..." -ForegroundColor Green

$WixCandidates = @(
    (Get-Command "wix.exe" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
    "C:\Program Files\WiX Toolset v7.0\bin\wix.exe",
    "$env:LOCALAPPDATA\Programs\WiX Toolset v7.0\bin\wix.exe"
)

$WixPath = $null
foreach ($path in $WixCandidates) {
    if ($path -and (Test-Path $path)) {
        $WixPath = $path
        break
    }
}

$MsiBuilt = $false
if ($WixPath) {
    Write-Host "      Using WiX at: $WixPath" -ForegroundColor Gray
    
    # Accept EULA if needed
    & $WixPath eula accept wix7 2>$null | Out-Null

    # Add UI extension globally if missing
    & $WixPath extension add WixToolset.UI.wixext -g 2>$null | Out-Null

    # Build Japanese MSI (Default)
    $MsiOutput = Join-Path $DistDir "hid-remap-setup.msi"
    $MsiOutputVersioned = Join-Path $DistDir "hid-remap-setup-v0.1.0.msi"
    $MsiOutputEn = Join-Path $DistDir "hid-remap-setup-en.msi"

    Set-Location $WorkspaceRoot
    & $WixPath build -arch x64 -ext WixToolset.UI.wixext -culture ja-JP $PackageWxs $PackageJaWxl -o $MsiOutput
    if ($LASTEXITCODE -eq 0) {
        Copy-Item -Path $MsiOutput -Destination $MsiOutputVersioned -Force
        $MsiBuilt = $true
        $msiSizeMb = [math]::Round((Get-Item $MsiOutput).Length / 1MB, 2)
        Write-Host "      Japanese MSI generated: $MsiOutput ($msiSizeMb MB)" -ForegroundColor Green
    } else {
        Write-Host "      Warning: Japanese MSI compilation failed." -ForegroundColor Yellow
    }

    # Build English MSI
    & $WixPath build -arch x64 -ext WixToolset.UI.wixext -culture en-US $PackageWxs $PackageEnWxl -o $MsiOutputEn
    if ($LASTEXITCODE -eq 0) {
        $msiEnSizeMb = [math]::Round((Get-Item $MsiOutputEn).Length / 1MB, 2)
        Write-Host "      English MSI generated:  $MsiOutputEn ($msiEnSizeMb MB)" -ForegroundColor Green
    }
} else {
    Write-Host "      WiX Toolset not found. Skipping MSI build. (Install via: winget install WiXToolset.WiXCLI)" -ForegroundColor Yellow
}

# 3. Build Setup Executable (.exe) with Inno Setup
Write-Host "[3/3] Building Inno Setup (.exe) installer..." -ForegroundColor Green
$InnoCandidates = @(
    (Get-Command "ISCC.exe" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
    "C:\Users\sgt23\Documents\Projects\teams_fullscreen_ext_v0.1.0\.tools\inno\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
)

$IsccPath = $null
foreach ($path in $InnoCandidates) {
    if ($path -and (Test-Path $path)) {
        $IsccPath = $path
        break
    }
}

if ($IsccPath -and (Test-Path $IssFile)) {
    Write-Host "      Using Inno Setup at: $IsccPath" -ForegroundColor Gray
    & $IsccPath $IssFile
    if ($LASTEXITCODE -eq 0) {
        $VersionedSetup = Join-Path $DistDir "hid-remap-setup-v0.1.0.exe"
        $GenericSetup = Join-Path $DistDir "hid-remap-setup.exe"
        if (Test-Path $VersionedSetup) {
            Copy-Item -Path $VersionedSetup -Destination $GenericSetup -Force
            $exeSizeMb = [math]::Round((Get-Item $GenericSetup).Length / 1MB, 2)
            Write-Host "      EXE installer generated: $GenericSetup ($exeSizeMb MB)" -ForegroundColor Green
        }
    }
} else {
    Write-Host "      Inno Setup not found. Skipping EXE build." -ForegroundColor Gray
}

Write-Host ""
Write-Host "==========================================================" -ForegroundColor Green
Write-Host "  Installer Build Complete!" -ForegroundColor Green
if (Test-Path (Join-Path $DistDir "hid-remap-setup.msi")) {
    Write-Host "  MSI Package (All Users / Current User):" -ForegroundColor White
    Write-Host "    $DistDir\hid-remap-setup.msi" -ForegroundColor Yellow
}
if (Test-Path (Join-Path $DistDir "hid-remap-setup.exe")) {
    Write-Host "  EXE Package:" -ForegroundColor White
    Write-Host "    $DistDir\hid-remap-setup.exe" -ForegroundColor Yellow
}
Write-Host "==========================================================" -ForegroundColor Green
