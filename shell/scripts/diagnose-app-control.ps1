# Diagnose why a toolchain binary cannot start (Windows Application Control).
#
# Read-only. Changes nothing, installs nothing, needs no elevation for most
# checks (a few need admin and are reported as "unknown" instead).
#
# Usage:
#   pwsh -NoProfile -File diagnose-app-control.ps1
#   pwsh -NoProfile -File diagnose-app-control.ps1 -ToolsPath 'C:\futureacademy\tools\Arduino'
#
# Produces a verdict naming the enforcing mechanism and the matching fix.

[CmdletBinding()]
param(
    [string]$ToolsPath = 'C:\futureacademy\tools\Arduino',
    [string]$Binary     = 'arduino-cli.exe'
)

$ErrorActionPreference = 'Continue'

function Section($title) {
    Write-Host ""
    Write-Host "=== $title ===" -ForegroundColor Cyan
}

$target = Join-Path $ToolsPath $Binary
$isAdmin = ([Security.Principal.WindowsPrincipal] `
    [Security.Principal.WindowsIdentity]::GetCurrent()
).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

Write-Host "Toolchain : $ToolsPath"
Write-Host "Binary    : $target"
Write-Host "Elevated  : $isAdmin"
Write-Host "Windows   : $((Get-CimInstance Win32_OperatingSystem).Caption) build $([Environment]::OSVersion.Version.Build)"

# ── 1. Is the file there, and is it signed? ────────────────────────────────
Section 'Binary and signature'
if (Test-Path -LiteralPath $target) {
    $item = Get-Item -LiteralPath $target
    $sig = Get-AuthenticodeSignature -LiteralPath $target
    Write-Host ("  size      : {0:N0} bytes" -f $item.Length)
    Write-Host ("  signature : {0}" -f $sig.Status)
    if ($sig.SignerCertificate) {
        Write-Host ("  signer    : {0}" -f $sig.SignerCertificate.Subject)
    }
    # Prove the OS actually refuses it, rather than guessing from the signature.
    try {
        $null = & $target version 2>&1
        Write-Host "  launch    : OK (the OS runs this binary)" -ForegroundColor Green
        $launchBlocked = $false
    } catch {
        Write-Host "  launch    : FAILED -> $($_.Exception.Message)" -ForegroundColor Red
        $launchBlocked = $true
    }
} else {
    Write-Host "  MISSING: $target" -ForegroundColor Red
}

# ── 2. Smart App Control (Windows 11) ─────────────────────────────────────
Section 'Smart App Control'
$sac = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy' `
    -Name VerifiedAndReputablePolicyState -ErrorAction SilentlyContinue
$sacState = if ($null -ne $sac) { $sac.VerifiedAndReputablePolicyState } else { $null }
switch ($sacState) {
    0       { Write-Host "  state: 0 (off)" }
    1       { Write-Host "  state: 1 (ENFORCING)" -ForegroundColor Yellow }
    2       { Write-Host "  state: 2 (evaluation)" -ForegroundColor Yellow }
    default { Write-Host "  state: not present (SAC is absent - Windows 10, or removed)" }
}

# ── 3. WDAC / App Control for Business ────────────────────────────────────
Section 'WDAC policies'
$policyDir = Join-Path $env:SystemRoot 'System32\CodeIntegrity\CiPolicies\Active'
$policies = @(Get-ChildItem -LiteralPath $policyDir -ErrorAction SilentlyContinue)
if ($policies.Count -eq 0) {
    Write-Host "  no active user-mode policies in $policyDir"
} else {
    foreach ($p in $policies) { Write-Host ("  {0}  ({1:N0} bytes)" -f $p.Name, $p.Length) }
}
$siPolicy = Join-Path $env:SystemRoot 'System32\CodeIntegrity\SiPolicy.p7b'
Write-Host ("  SiPolicy.p7b: {0}" -f $(if (Test-Path -LiteralPath $siPolicy) { 'present (base policy deployed)' } else { 'absent' }))

# ── 4. AppLocker ──────────────────────────────────────────────────────────
Section 'AppLocker'
try {
    $al = Get-AppLockerPolicy -Effective -ErrorAction Stop
    $xml = $al.ToXml()
    if ($xml -match 'RuleCollection') {
        Write-Host "  effective policy present; rules matching this toolchain:"
        $hits = @([regex]::Matches($xml, '(?is)<FilePathRule.*?</FilePathRule>') |
            Where-Object { $_.Value -match 'Arduino|FutureAcademy|futureacademy' })
        if ($hits.Count -eq 0) { Write-Host "    (none reference this toolchain)" }
        foreach ($h in $hits) { Write-Host ("    {0}" -f ($h.Value -replace '\s+', ' ').Substring(0, [Math]::Min(200, $h.Value.Length))) }
    } else {
        Write-Host "  no effective AppLocker policy"
    }
} catch {
    Write-Host "  could not read effective policy: $($_.Exception.Message)"
}

# ── 5. Defender ASR / ISG ─────────────────────────────────────────────────
Section 'Defender attack surface reduction'
try {
    $mp = Get-MpPreference -ErrorAction Stop
    $ids = @($mp.AttackSurfaceReductionRules_Ids | Where-Object { $_ })
    $acts = @($mp.AttackSurfaceReductionRules_Actions)
    if ($ids.Count -eq 0) {
        Write-Host "  no ASR rules configured"
    } else {
        # 01443614 is "block executables that do not meet prevalence/age/trust".
        $isgId = '01443614-cd74-433a-b99e-2ecdc07bfc25'
        for ($i = 0; $i -lt $ids.Count; $i++) {
            $action = if ($i -lt $acts.Count) { $acts[$i] } else { '?' }
            $flag = if ($ids[$i] -eq $isgId) { '  <-- blocks untrusted executables' } else { '' }
            Write-Host ("  {0}  action={1}{2}" -f $ids[$i], $action, $flag)
        }
    }
} catch {
    Write-Host "  could not read Defender preferences: $($_.Exception.Message)"
}

# ── 6. CodeIntegrity log: names the policy that actually blocked us ───────
Section 'CodeIntegrity operational log (authoritative)'
try {
    $events = Get-WinEvent -LogName 'Microsoft-Windows-CodeIntegrity/Operational' -MaxEvents 60 -ErrorAction Stop |
        Where-Object { $_.Message -match 'arduino|xtensa|futureacademy' }
    if (-not $events) {
        Write-Host "  no recent block events for this toolchain" -ForegroundColor Green
    } else {
        foreach ($e in $events | Select-Object -First 5) {
            Write-Host ("  [{0}] id={1}" -f $e.TimeCreated, $e.Id) -ForegroundColor Yellow
            Write-Host ("    {0}" -f (($e.Message -split "`n")[0..2] -join ' '))
        }
    }
} catch {
    Write-Host "  log unavailable: $($_.Exception.Message)"
}

# ── 7. Verdict ────────────────────────────────────────────────────────────
Section 'VERDICT'
$mechanisms = @()
if ($sacState -eq 1 -or $sacState -eq 2) { $mechanisms += 'Smart App Control' }
if ($policies.Count -gt 0 -or (Test-Path -LiteralPath $siPolicy)) { $mechanisms += 'WDAC' }
if ($mechanisms.Count -eq 0) { $mechanisms += 'AppLocker or ASR/ISG (see sections above)' }

if (-not $launchBlocked) {
    Write-Host "  The OS starts this binary. Application Control is NOT blocking it on this machine." -ForegroundColor Green
    Write-Host "  If uploads still fail, the cause is elsewhere - see docs/upload-workflow.md."
} else {
    Write-Host ("  Binary is refused. Likely mechanism: {0}" -f ($mechanisms -join ', ')) -ForegroundColor Yellow
    Write-Host ""
    Write-Host "  Fix, in order of durability:"
    if ($mechanisms -contains 'Smart App Control') {
        Write-Host "    * SAC has no allowlist. Signing with your own certificate will NOT help."
        Write-Host "      Options: turn SAC off (irreversible without reinstalling Windows), or"
        Write-Host "      compile on the server so this machine never runs the toolchain."
    }
    if ($mechanisms -contains 'WDAC') {
        Write-Host "    * Sign every toolchain binary, then deploy a supplemental WDAC policy"
        Write-Host "      with a Publisher rule for that certificate."
        Write-Host "      Helpers: shell/scripts/sign-toolchain.ps1"
    }
    Write-Host "    * Or grant an AppLocker path rule / Defender exclusion for:"
    Write-Host "      $ToolsPath"
    Write-Host ""
    Write-Host "  Details and commands: docs/windows-app-control.md"
}
Write-Host ""
