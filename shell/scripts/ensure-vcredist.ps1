#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Ensures that the Microsoft Visual C++ Redistributable 2015-2022 (x64) is installed.

.DESCRIPTION
    esptool.exe (the ESP32 flash utility bundled with Future Academy) is compiled with
    PyInstaller and requires VCRUNTIME140.dll and the Universal CRT DLLs
    (api-ms-win-crt-*.dll / ucrtbase.dll).

    Both are provided by the Microsoft Visual C++ 2015-2022 Redistributable (x64).

    This script:
      1. Checks the registry to see if the runtime is already present.
      2. If missing, downloads vc_redist.x64.exe from Microsoft's official aka.ms URL.
      3. Installs it silently with /install /quiet /norestart.

.EXAMPLE
    # Run from an elevated PowerShell prompt:
    .\ensure-vcredist.ps1

.EXAMPLE
    # Call from another script (returns $true on success):
    $ok = & "$PSScriptRoot\ensure-vcredist.ps1"; if (-not $ok) { exit 1 }
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# 1. Detection
# ---------------------------------------------------------------------------
function Test-VCRedist2022x64 {
    $keys = @(
        'HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x64'
    )
    foreach ($key in $keys) {
        if (Test-Path $key) {
            $val = (Get-ItemProperty -Path $key -Name 'Installed' -ErrorAction SilentlyContinue).Installed
            if ($val -eq 1) {
                return $true
            }
        }
    }
    return $false
}

# ---------------------------------------------------------------------------
# 2. Download
# ---------------------------------------------------------------------------
function Get-VCRedist {
    param([string]$DestPath)

    $url = 'https://aka.ms/vs/17/release/vc_redist.x64.exe'
    Write-Host "[vcredist] Downloading from: $url" -ForegroundColor Cyan

    # Force TLS 1.2 for older PowerShell 5 sessions
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

    try {
        Invoke-WebRequest -Uri $url -OutFile $DestPath -UseBasicParsing -TimeoutSec 120
        return Test-Path $DestPath
    } catch {
        Write-Warning "[vcredist] Download failed: $_"
        return $false
    }
}

# ---------------------------------------------------------------------------
# 3. Install
# ---------------------------------------------------------------------------
function Install-VCRedist {
    param([string]$InstallerPath)

    Write-Host "[vcredist] Installing Visual C++ 2015-2022 Redistributable (x64)..." -ForegroundColor Cyan
    $proc = Start-Process -FilePath $InstallerPath `
                          -ArgumentList '/install', '/quiet', '/norestart' `
                          -Wait -PassThru -NoNewWindow
    $code = $proc.ExitCode

    # 0 = success, 1638 = newer/equal version already exists, 3010 = reboot needed
    if ($code -in 0, 1638, 3010) {
        Write-Host "[vcredist] Installed successfully (exit code $code)." -ForegroundColor Green
        if ($code -eq 3010) {
            Write-Warning "[vcredist] A system restart is recommended to complete the installation."
        }
        return $true
    }

    Write-Warning "[vcredist] Installer exited with unexpected code: $code"
    Write-Warning "[vcredist] You can install manually from: https://aka.ms/vs/17/release/vc_redist.x64.exe"
    return $false
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
if (Test-VCRedist2022x64) {
    Write-Host "[vcredist] Visual C++ 2015-2022 Redistributable (x64) is already installed." -ForegroundColor Green
    exit 0
}

Write-Host "[vcredist] Visual C++ 2015-2022 Redistributable (x64) not found." -ForegroundColor Yellow

$tempFile = Join-Path $env:TEMP 'vc_redist.x64.exe'

if (-not (Get-VCRedist -DestPath $tempFile)) {
    Write-Error "[vcredist] Could not download the installer. Check your internet connection."
    exit 1
}

$ok = Install-VCRedist -InstallerPath $tempFile

# Cleanup download
if (Test-Path $tempFile) {
    Remove-Item $tempFile -Force -ErrorAction SilentlyContinue
}

if (-not $ok) {
    exit 1
}

exit 0
