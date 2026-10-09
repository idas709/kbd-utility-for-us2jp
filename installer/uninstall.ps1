#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Uninstalls HID Keyboard Layout Remapper and restores clean keyboard drivers.
#>

$ErrorActionPreference = "Stop"

Write-Host "==========================================================" -ForegroundColor Yellow
Write-Host "  HID Keyboard Layout Remapper - Uninstaller" -ForegroundColor Yellow
Write-Host "==========================================================" -ForegroundColor Yellow

$InstallDir = "$env:ProgramFiles\HID Keyboard Remapper"
$ServiceName = "HidKeyboardRemapper"

# 1. Stop and remove User Session Agent
Write-Host "[1/4] Stopping User Session Agent..." -ForegroundColor Green
Get-Process -Name "hid-remap-agent" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process -Name "hid-remap-gui" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
Remove-ItemProperty -Path $RunKey -Name "HidKeyboardRemapperAgent" -ErrorAction SilentlyContinue

# 2. Stop and remove Service
Write-Host "[2/4] Stopping and removing Windows Service..." -ForegroundColor Green
$existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($existing) {
    Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
    sc.exe delete $ServiceName | Out-Null
}

# 3. Clean up Keyboard UpperFilters in Registry if installed
Write-Host "[3/4] Restoring clean keyboard UpperFilters in registry..." -ForegroundColor Green
$ClassKey = "HKLM:\System\CurrentControlSet\Control\Class\{4d36e96b-e325-11ce-bfc1-08002be10318}"
if (Test-Path $ClassKey) {
    $filters = (Get-ItemProperty -Path $ClassKey).UpperFilters
    if ($filters) {
        $cleaned = @($filters | Where-Object { $_ -ne "kbfiltr" })
        if ($cleaned.Count -eq 0) {
            # Standard default is usually empty or none
            Remove-ItemProperty -Path $ClassKey -Name "UpperFilters" -ErrorAction SilentlyContinue
        } else {
            Set-ItemProperty -Path $ClassKey -Name "UpperFilters" -Value $cleaned
        }
    }
}

# 4. Remove installation files
Write-Host "[4/4] Removing installation directory..." -ForegroundColor Green
if (Test-Path $InstallDir) {
    Remove-Item -Path $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "==========================================================" -ForegroundColor Green
Write-Host "  Uninstallation completed. Standard keyboard drivers restored." -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green

