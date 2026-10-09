#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Installs the HID Keyboard Layout Remapper (Service, Agent, GUI, and Driver).
#>

$ErrorActionPreference = "Stop"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  HID Keyboard Layout Remapper - Installer" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$InstallDir = "$env:ProgramFiles\HID Keyboard Remapper"
$SourceDist = Join-Path $PSScriptRoot "..\dist"

if (-not (Test-Path $SourceDist)) {
    Write-Host "Dist directory not found. Please run build-release.ps1 first." -ForegroundColor Yellow
    exit 1
}

Write-Host "[1/5] Creating installation directory at '$InstallDir'..." -ForegroundColor Green
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

Write-Host "[2/5] Copying binaries and configuration..." -ForegroundColor Green
Copy-Item -Path "$SourceDist\*" -Destination $InstallDir -Recurse -Force

# Create default config if not exists
$ConfigFile = Join-Path $InstallDir "config.toml"
if (-not (Test-Path $ConfigFile)) {
    Copy-Item -Path (Join-Path $PSScriptRoot "..\config.toml") -Destination $ConfigFile -Force
}

Write-Host "[3/5] Registering Windows Service (HidKeyboardRemapper)..." -ForegroundColor Green
$ServiceExe = Join-Path $InstallDir "hid-remap-service.exe"
$ServiceName = "HidKeyboardRemapper"

$existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($existing) {
    Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
    sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Seconds 1
}

sc.exe create $ServiceName binPath= "`"$ServiceExe`"" start= auto DisplayName= "HID Keyboard Layout Remapper Service" | Out-Null
sc.exe description $ServiceName "Manages per-device keyboard layout remapping and IME switching." | Out-Null

Write-Host "[4/5] Registering User Session Agent to Windows Startup..." -ForegroundColor Green
$AgentExe = Join-Path $InstallDir "hid-remap-agent.exe"
$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
Set-ItemProperty -Path $RunKey -Name "HidKeyboardRemapperAgent" -Value "`"$AgentExe`""

Write-Host "[5/5] Starting Service and User Agent..." -ForegroundColor Green
Start-Service -Name $ServiceName -ErrorAction SilentlyContinue
Start-Process -FilePath $AgentExe -WindowStyle Hidden -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "==========================================================" -ForegroundColor Green
Write-Host "  Installation completed successfully!" -ForegroundColor Green
Write-Host "  You can launch the configuration GUI at:" -ForegroundColor White
Write-Host "  $InstallDir\hid-remap-gui.exe" -ForegroundColor Yellow
Write-Host "==========================================================" -ForegroundColor Green

