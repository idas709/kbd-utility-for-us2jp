<#
.SYNOPSIS
    Script-based installer for HID Keyboard Layout Remapper (Standalone User-Mode).
    For the graphical wizard installer, run dist\hid-remap-setup.exe.
#>

$ErrorActionPreference = "Stop"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  HID Keyboard Layout Remapper - Portable / Script Setup" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# Install to user local programs by default (No Admin / UAC elevation needed)
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\HID Keyboard Layout Remapper"
$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$SourceDist = Join-Path $WorkspaceRoot "dist"

if (-not (Test-Path (Join-Path $SourceDist "hid-remap.exe"))) {
    $SourceDist = Join-Path $WorkspaceRoot "target\release"
}

if (-not (Test-Path (Join-Path $SourceDist "hid-remap.exe"))) {
    Write-Host "Error: hid-remap.exe not found! Run 'cargo build --release -p hid-remap-app' first." -ForegroundColor Red
    exit 1
}

# 1. Stop running instance if any
Write-Host "[1/4] Checking for running instances..." -ForegroundColor Green
Get-Process -Name "hid-remap" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# 2. Copy binaries and config
Write-Host "[2/4] Installing application to '$InstallDir'..." -ForegroundColor Green
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

Copy-Item -Path (Join-Path $SourceDist "hid-remap.exe") -Destination $InstallDir -Force

$ConfigSrc = Join-Path $WorkspaceRoot "config.toml"
$ConfigDest = Join-Path $InstallDir "config.toml"
if ((Test-Path $ConfigSrc) -and (-not (Test-Path $ConfigDest))) {
    Copy-Item -Path $ConfigSrc -Destination $ConfigDest -Force
}

$IconSrc = Join-Path $WorkspaceRoot "crates\hid-remap-app\ui\assets\app_icon.ico"
if (Test-Path $IconSrc) {
    Copy-Item -Path $IconSrc -Destination (Join-Path $InstallDir "app_icon.ico") -Force
}

# 3. Create Start Menu shortcut
Write-Host "[3/4] Creating Start Menu shortcut..." -ForegroundColor Green
$ProgramsDir = [Environment]::GetFolderPath("Programs")
$ShortcutPath = Join-Path $ProgramsDir "HID Keyboard Layout Remapper.lnk"
$WshShell = New-Object -ComObject WScript.Shell
$Shortcut = $WshShell.CreateShortcut($ShortcutPath)
$Shortcut.TargetPath = Join-Path $InstallDir "hid-remap.exe"
$Shortcut.WorkingDirectory = $InstallDir
$Shortcut.IconLocation = Join-Path $InstallDir "hid-remap.exe,0"
$Shortcut.Description = "HID Keyboard Layout Remapper"
$Shortcut.Save()

# 4. Register to Windows Startup (Run key)
Write-Host "[4/4] Registering Windows Startup..." -ForegroundColor Green
$ExePath = Join-Path $InstallDir "hid-remap.exe"
$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
Set-ItemProperty -Path $RunKey -Name "HIDKeyboardLayoutRemapper" -Value "`"$ExePath`""

Write-Host ""
Write-Host "==========================================================" -ForegroundColor Green
Write-Host "  Installation completed successfully!" -ForegroundColor Green
Write-Host "  Installed at: $InstallDir" -ForegroundColor White
Write-Host "  To launch immediately, run:" -ForegroundColor White
Write-Host "  Start-Process `"$ExePath`"" -ForegroundColor Yellow
Write-Host "==========================================================" -ForegroundColor Green
