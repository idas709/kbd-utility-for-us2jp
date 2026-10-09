<#
.SYNOPSIS
    Script-based uninstaller for HID Keyboard Layout Remapper.
#>

$ErrorActionPreference = "Stop"

Write-Host "==========================================================" -ForegroundColor Yellow
Write-Host "  HID Keyboard Layout Remapper - Uninstaller" -ForegroundColor Yellow
Write-Host "==========================================================" -ForegroundColor Yellow

$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\HID Keyboard Layout Remapper"

# 1. Stop running process
Write-Host "[1/3] Terminating running process..." -ForegroundColor Green
Get-Process -Name "hid-remap" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# 2. Remove Shortcuts & Registry entries
Write-Host "[2/3] Cleaning up shortcuts and startup registry..." -ForegroundColor Green
$ProgramsDir = [Environment]::GetFolderPath("Programs")
$ShortcutPath = Join-Path $ProgramsDir "HID Keyboard Layout Remapper.lnk"
if (Test-Path $ShortcutPath) {
    Remove-Item -Path $ShortcutPath -Force -ErrorAction SilentlyContinue
}

$DesktopDir = [Environment]::GetFolderPath("Desktop")
$DesktopShortcut = Join-Path $DesktopDir "HID Keyboard Layout Remapper.lnk"
if (Test-Path $DesktopShortcut) {
    Remove-Item -Path $DesktopShortcut -Force -ErrorAction SilentlyContinue
}

$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
Remove-ItemProperty -Path $RunKey -Name "HIDKeyboardLayoutRemapper" -ErrorAction SilentlyContinue

# 3. Remove Installed files
Write-Host "[3/3] Removing installation files..." -ForegroundColor Green
if (Test-Path $InstallDir) {
    Remove-Item -Path $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "==========================================================" -ForegroundColor Green
Write-Host "  Uninstallation completed successfully." -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green
