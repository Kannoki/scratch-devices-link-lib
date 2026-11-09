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

# ── 1. Signature, and whether the OS actually starts each launcher ─────────
# NB: never probe with `& $exe version 2>&1` inside try/catch. A refused launch
# is written to the error *stream*, and `2>&1` turns it into a pipeline object,
# so no exception is raised and the probe reports a false success. Start-Process
# throws for real when CreateProcess is denied.
function Test-Launcher {
    param([string]$Path, [string[]]$Arguments)
    if (-not (Test-Path -LiteralPath $Path)) {
        return [pscustomobject]@{ Present = $false; Ran = $false; Error = 'missing'; Info = '' }
    }
    $outFile = [System.IO.Path]::GetTempFileName()
    $errFile = [System.IO.Path]::GetTempFileName()
    try {
        $p = Start-Process -FilePath $Path -ArgumentList $Arguments -Wait -PassThru `
            -NoNewWindow -RedirectStandardOutput $outFile -RedirectStandardError $errFile `
            -ErrorAction Stop
        $text = (@(Get-Content -LiteralPath $outFile -ErrorAction SilentlyContinue) +
                  @(Get-Content -LiteralPath $errFile -ErrorAction SilentlyContinue)) |
            Where-Object { $_ -and $_.Trim() } | Select-Object -First 1
        if ($p.ExitCode -eq 0) {
            return [pscustomobject]@{ Present = $true; Ran = $true; Error = ''; Info = "$text" }
        }
        return [pscustomobject]@{ Present = $true; Ran = $false; Error = "exit code $($p.ExitCode)"; Info = '' }
    } catch {
        return [pscustomobject]@{ Present = $true; Ran = $false; Error = $_.Exception.Message; Info = '' }
    } finally {
        Remove-Item -LiteralPath $outFile, $errFile -Force -ErrorAction SilentlyContinue
    }
}

Section 'Launchers the build and flash need'
if (Test-Path -LiteralPath $target) {
    $item = Get-Item -LiteralPath $target
    $sig = Get-AuthenticodeSignature -LiteralPath $target
    Write-Host ("  {0}: {1:N0} bytes, signature {2}" -f $Binary, $item.Length, $sig.Status)
    if ($sig.SignerCertificate) {
        Write-Host ("    signer: {0}" -f $sig.SignerCertificate.Subject)
    }
} else {
    Write-Host "  MISSING: $target" -ForegroundColor Red
}

$espRoot = Join-Path $ToolsPath 'packages\esp32\tools'
$probes = @(
    [pscustomobject]@{ Name = 'arduino-cli (compile + upload)'; Path = $target; Arguments = @('version') }
)
$esptoolFile = Get-ChildItem -LiteralPath (Join-Path $espRoot 'esptool_py') -Recurse -Filter 'esptool.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($esptoolFile) {
    $probes += [pscustomobject]@{ Name = 'esptool (flash only)'; Path = $esptoolFile.FullName; Arguments = @('version') }
}
$gppFile = Get-ChildItem -LiteralPath $espRoot -Recurse -Filter 'xtensa-*-g++.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($gppFile) {
    $probes += [pscustomobject]@{ Name = 'xtensa g++ (compile only)'; Path = $gppFile.FullName; Arguments = @('--version') }
}

$probeResults = @{}
foreach ($probe in $probes) {
    $r = Test-Launcher -Path $probe.Path -Arguments $probe.Arguments
    $probeResults[$probe.Name] = $r
    $status = if ($r.Ran) { 'RUNS' } else { 'BLOCKED' }
    $color = if ($r.Ran) { 'Green' } else { 'Red' }
    Write-Host ("  {0,-34} {1}  {2}" -f $probe.Name, $status, $r.Error) -ForegroundColor $color
    if ($r.Ran -and $r.Info) { Write-Host ("      {0}" -f $r.Info) -ForegroundColor DarkGray }
}

$cliBlocked = -not $probeResults['arduino-cli (compile + upload)'].Ran
$esptoolBlocked = $probeResults.ContainsKey('esptool (flash only)') -and -not $probeResults['esptool (flash only)'].Ran
$compilerBlocked = $probeResults.ContainsKey('xtensa g++ (compile only)') -and -not $probeResults['xtensa g++ (compile only)'].Ran
$launchBlocked = $cliBlocked -or $esptoolBlocked -or $compilerBlocked

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

# ── 3b. Who owns this policy? MDM / domain management decides who can change it.
Section 'Management (who can change the policy)'
$mdm = @(Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Enrollments' -ErrorAction SilentlyContinue |
    ForEach-Object { Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue } |
    Where-Object { $_.ProviderID -and $_.ProviderID -ne 'None' })
if ($mdm.Count -gt 0) {
    Write-Host ("  enrolled to MDM ({0} enrollment(s)):" -f $mdm.Count) -ForegroundColor Yellow
    $mdm | ForEach-Object {
        if ($_.EnrollmentState -eq 1) { Write-Host ("    ProviderID={0}  MDMUrl={1}" -f $_.ProviderID, $_.DiscoveryServiceFullURL) }
    }
} else {
    Write-Host "  no MDM enrollment found"
}
$domain = (Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue).PartOfDomain
Write-Host ("  domain joined: {0}" -f $domain)
Write-Host "  -> A WDAC policy on a managed machine can only be changed by its owner."

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
    Write-Host "  All three launchers start. Application Control is NOT blocking them on this machine." -ForegroundColor Green
    Write-Host "  If uploads still fail, the cause is elsewhere - see docs/upload-workflow.md."
} else {
    Write-Host ("  Blocked launcher(s). Likely mechanism: {0}" -f ($mechanisms -join ', ')) -ForegroundColor Yellow
    Write-Host ""
    Write-Host "  What is still possible on this machine:"
    if ($compilerBlocked) {
        Write-Host "    * The compiler itself is refused, so NOTHING can be compiled locally." -ForegroundColor Yellow
        Write-Host "      Local signing + a WDAC Publisher rule for your own certificate is the only"
        Write-Host "      in-place fix; it requires the policy owner to add that signer."
    }
    if ($esptoolBlocked) {
        Write-Host "    * esptool is refused too, so this machine cannot flash either." -ForegroundColor Yellow
        Write-Host "      Only a policy change helps - nothing in the app can work around it."
    } elseif ($probeResults.ContainsKey('esptool (flash only)')) {
        Write-Host "    * esptool still runs: flashing works." -ForegroundColor Green
        Write-Host "      Compiling on the server and flashing locally is viable for this machine"
        Write-Host "      (see docs/compile-server-deployment.md)."
    }
    Write-Host ""
    Write-Host "  Fix, in order of durability:"
    if ($mechanisms -contains 'WDAC') {
        Write-Host "    * WDAC is enforcing. A block event naming 'Enterprise signing level' means the"
        Write-Host "      policy requires a signing level the unsigned toolchain cannot satisfy."
        Write-Host "      The policy owner must add a rule for our signer (Publisher rule after"
        Write-Host "      signing), or place the toolchain inside an already-trusted install path."
        Write-Host "      Helpers: shell/scripts/sign-toolchain.ps1, docs/windows-app-control.md"
    }
    if ($mechanisms -contains 'Smart App Control') {
        Write-Host "    * SAC has no allowlist. Signing with your own certificate will NOT help."
        Write-Host "      Options: turn SAC off (irreversible without reinstalling Windows), or"
        Write-Host "      compile on the server so this machine never runs the toolchain."
    }
    Write-Host "    * Last resort for an unmanaged machine: an AppLocker path rule or a Defender"
    Write-Host "      exclusion for $ToolsPath - weaker protection, admin consent required."
    Write-Host ""
    Write-Host "  Details and commands: docs/windows-app-control.md"
}
Write-Host ""
