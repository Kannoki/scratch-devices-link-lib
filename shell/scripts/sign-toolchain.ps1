# Sign every executable in the shipped toolchain so Windows Application Control
# (WDAC / AppLocker publisher rules) can trust it.
#
# MUST run at package time, before the tools archive is built: the app replaces
# the whole Arduino/ tree when it refreshes the toolchain, so signatures applied
# on an end-user machine are lost at the next update. See docs/windows-app-control.md.
#
# Examples:
#   pwsh -NoProfile -File sign-toolchain.ps1 -ToolsPath tools/Arduino -Thumbprint <SHA1> -VerifyOnly
#   pwsh -NoProfile -File sign-toolchain.ps1 -ToolsPath tools/Arduino -PfxPath .\signing.pfx -PfxPassword $p
#
# Requires signtool.exe from the Windows SDK.

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ToolsPath,
    [string]$Thumbprint,
    [string]$PfxPath,
    [string]$PfxPassword,
    [string]$TimestampUrl = 'http://timestamp.digicert.com',
    [switch]$IncludeDll,
    [switch]$VerifyOnly
)

$ErrorActionPreference = 'Stop'

function Find-SignTool {
    $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $kits = 'C:\Program Files (x86)\Windows Kits\10\bin'
    if (Test-Path -LiteralPath $kits) {
        $found = Get-ChildItem -LiteralPath $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\x64\\' } |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($found) { return $found.FullName }
    }
    throw 'signtool.exe not found. Install the Windows 10/11 SDK (Signing Tools).'
}

if (-not (Test-Path -LiteralPath $ToolsPath)) {
    throw "ToolsPath not found: $ToolsPath"
}

$patterns = if ($IncludeDll) { @('*.exe', '*.dll') } else { @('*.exe') }
$files = foreach ($p in $patterns) {
    Get-ChildItem -LiteralPath $ToolsPath -Recurse -File -Filter $p -ErrorAction SilentlyContinue
}
$files = @($files)
if ($files.Count -eq 0) { throw "No binaries found under $ToolsPath" }

Write-Host "Found $($files.Count) binaries under $ToolsPath" -ForegroundColor Cyan

if ($VerifyOnly) {
    $results = $files | ForEach-Object { Get-AuthenticodeSignature -LiteralPath $_.FullName }
    $grouped = $results | Group-Object Status
    foreach ($g in $grouped) { Write-Host ("  {0,-12} {1}" -f $g.Name, $g.Count) }
    $bad = @($results | Where-Object { $_.Status -ne 'Valid' })
    if ($bad.Count -gt 0) {
        Write-Host "UNSIGNED/INVALID: $($bad.Count) file(s)" -ForegroundColor Red
        $bad | Select-Object -First 10 | ForEach-Object { Write-Host "  $($_.Path)" }
        exit 1
    }
    Write-Host "All binaries carry a valid Authenticode signature." -ForegroundColor Green
    exit 0
}

$signtool = Find-SignTool
Write-Host "signtool: $signtool"

# Resolve the certificate arguments.
$certArgs = @()
if ($Thumbprint) {
    $cert = Get-ChildItem Cert:\CurrentUser\My, Cert:\LocalMachine\My -ErrorAction SilentlyContinue |
        Where-Object { $_.Thumbprint -eq $Thumbprint } | Select-Object -First 1
    if (-not $cert) { throw "No certificate with thumbprint $Thumbprint in CurrentUser\My or LocalMachine\My" }
    $certArgs = @('/sha1', $Thumbprint)
} elseif ($PfxPath) {
    if (-not (Test-Path -LiteralPath $PfxPath)) { throw "PFX not found: $PfxPath" }
    $certArgs = @('/f', (Resolve-Path -LiteralPath $PfxPath).Path)
    if ($PfxPassword) { $certArgs += @('/p', $PfxPassword) }
} else {
    throw 'Supply -Thumbprint or -PfxPath (a code-signing certificate is required).'
}

$failed = 0
foreach ($file in $files) {
    $signArgs = @('sign', '/fd', 'SHA256', '/tr', $TimestampUrl, '/td', 'SHA256') + $certArgs + @($file.FullName)
    & $signtool @signArgs | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  FAILED: $($file.FullName)" -ForegroundColor Red
        $failed++
    }
}

Write-Host "Signed $($files.Count - $failed)/$($files.Count) binaries" -ForegroundColor Cyan
if ($failed -gt 0) { throw "$failed binary/binaries could not be signed." }

# Verify immediately - a silent signing failure is worse than none.
$bad = @($files | ForEach-Object { Get-AuthenticodeSignature -LiteralPath $_.FullName } |
    Where-Object { $_.Status -ne 'Valid' })
if ($bad.Count -gt 0) {
    Write-Host "VERIFY FAILED for $($bad.Count) file(s):" -ForegroundColor Red
    $bad | Select-Object -First 10 | ForEach-Object { Write-Host "  $($_.Path) -> $($_.Status)" }
    exit 1
}
Write-Host "All signatures verified." -ForegroundColor Green

# Emit the policy commands so the trust side is not left to guesswork.
$sample = $files[0].FullName
Write-Host ""
Write-Host "Next: trust this signer on the target machines." -ForegroundColor Cyan
Write-Host "  WDAC (run on a admin workstation, deploy the .cip via GPO):"
Write-Host "    New-CIPolicyRule -Level Publisher -DriverFilePath '$sample' -OutputFilePath .\toolchain-rule.xml"
Write-Host "    New-CIPolicy -PolicyName 'FutureAcademyToolchainTrust' -Rule .\toolchain-rule.xml -FilePath .\ToolchainTrust.xml"
Write-Host "    ConvertFrom-CIPolicy .\ToolchainTrust.xml .\ToolchainTrust.cip"
Write-Host ""
Write-Host "  AppLocker publisher rule:"
Write-Host "    New-AppLockerPolicy -RuleType Publisher -User Everyone -Path '$sample' -RuleNamePrefix FutureAcademy | Set-AppLockerPolicy -Merge"
Write-Host ""
Write-Host "  Remember: Smart App Control does not honour either rule." -ForegroundColor Yellow
