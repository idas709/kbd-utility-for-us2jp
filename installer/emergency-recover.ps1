#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Emergency recovery script to restore standard Windows keyboard driver stack
    in case of any driver fault, crash, or input lockup (AT-19, AT-23).
#>

Write-Host "==========================================================" -ForegroundColor Red
Write-Host "  EMERGENCY RECOVERY: Standard Keyboard Driver Stack" -ForegroundColor Red
Write-Host "==========================================================" -ForegroundColor Red

# Stop service and processes immediately
Write-Host "Stopping any running services or agents..." -ForegroundColor Yellow
Stop-Service -Name "HidKeyboardRemapper" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "hid-remap-agent" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "hid-remap-service" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "hid-remap-gui" -Force -ErrorAction SilentlyContinue

# Restore clean keyboard class UpperFilters
$ClassKey = "HKLM:\System\CurrentControlSet\Control\Class\{4d36e96b-e325-11ce-bfc1-08002be10318}"
Write-Host "Inspecting keyboard class filter key: $ClassKey" -ForegroundColor Yellow

if (Test-Path $ClassKey) {
    $current = (Get-ItemProperty -Path $ClassKey).UpperFilters
    Write-Host "Current UpperFilters: $($current -join ', ')" -ForegroundColor White

    if ($current) {
        $safeFilters = @($current | Where-Object { $_ -ne "kbfiltr" })
        if ($safeFilters.Count -eq 0) {
            Remove-ItemProperty -Path $ClassKey -Name "UpperFilters" -ErrorAction SilentlyContinue
            Write-Host "Removed custom UpperFilters entry." -ForegroundColor Green
        } else {
            Set-ItemProperty -Path $ClassKey -Name "UpperFilters" -Value $safeFilters
            Write-Host "Restored safe UpperFilters: $($safeFilters -join ', ')" -ForegroundColor Green
        }
    }
}

Write-Host ""
Write-Host "Keyboard drivers restored to Windows default state." -ForegroundColor Green
Write-Host "If inputs were locked, please unplug and reconnect your keyboard or reboot." -ForegroundColor White
Write-Host "==========================================================" -ForegroundColor Green

