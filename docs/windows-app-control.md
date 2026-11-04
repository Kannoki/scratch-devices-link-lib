# Windows Application Control blocks the toolchain

> Related: [#25](https://github.com/Kannoki/scratch-devices-link-lib/issues/25)

## Symptom

Upload fails before anything reaches the board:

```
failed to start C:\futureacademy\tools\Arduino/arduino-cli.exe:
An Application Control policy has blocked this file. (os error 4551)
```

`4551` is `ERROR_APPCONTROL_BLOCKED`. The failure is an **operating system policy
decision**, not a cable, driver, or board fault. `arduino-cli` never starts, so
the sketch is never compiled and nothing can be flashed.

## Why it happens

The shipped toolchain is third-party and almost entirely unsigned — in the
bundled ESP32 core 3.1.3, **132 of 140 executables carry no Authenticode
signature**, including `arduino-cli.exe` and the whole compiler
(`xtensa-esp32s3-elf-g++`, `gcc`, `objcopy`). Only the Espressif esptool family
and the `dpinst` driver installers are signed.

Windows Application Control refuses unsigned images when it is enforcing:

| Mechanism | Typical origin |
|---|---|
| Smart App Control | Windows 11 default on many consumer installs |
| WDAC (App Control for Business) | School / enterprise management |
| AppLocker | Domain policy |
| Defender ASR / ISG | "Block executable files unless they meet prevalence/age/trust" |

## Diagnose the affected machine

```powershell
Get-WinEvent -LogName 'Microsoft-Windows-CodeIntegrity/Operational' -MaxEvents 20 |
  Where-Object { $_.Message -match 'arduino|xtensa|futureacademy' } |
  Format-List TimeCreated, Id, Message

Get-ChildItem 'C:\Windows\System32\CodeIntegrity\CiPolicies\Active'
Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy' -Name VerifiedAndReputablePolicyState
Get-AppLockerPolicy -Effective -Xml 2>$null | Select-String 'arduino','futureacademy'
Get-AuthenticodeSignature "C:\futureacademy\tools\Arduino\arduino-cli.exe" | Format-List Status
```

The CodeIntegrity operational log names the policy GUID that produced the block.
**Identify the mechanism first** — the remediation differs, and signing does not
help with every mechanism.

## Fixes by mechanism

### AppLocker — path rule (simplest, no signing)

```powershell
New-AppLockerPolicy -RuleType Path -User Everyone `
  -Path 'C:\futureacademy\tools\Arduino\arduino-cli.exe' -RuleNamePrefix FutureAcademy |
  Set-AppLockerPolicy -Merge
```

Path rules are weaker than publisher rules: anything written to that folder can
run. Prefer publisher rules once the toolchain is signed.

### WDAC — signer rule (needs a signing certificate)

```powershell
New-CIPolicyRule -Level Publisher -DriverFilePath 'C:\futureacademy\tools\Arduino\arduino-cli.exe' `
  -OutputFilePath .\toolchain-rule.xml
New-CIPolicy -PolicyName "FutureAcademyToolchainTrust" -Rule .\toolchain-rule.xml -FilePath .\ToolchainTrust.xml
ConvertFrom-CIPolicy .\ToolchainTrust.xml .\ToolchainTrust.cip
```

A **publisher** rule covers every binary signed by the same certificate, so one
rule trusts the whole toolchain. If the machine already carries a base policy,
this must be deployed as a **supplemental** policy through GPO — copying the
`.cip` by hand into `CiPolicies\Active` is often rejected or ignored.

### Defender ASR / ISG

```powershell
Add-MpPreference -AttackSurfaceReductionOnlyExclusions "C:\futureacademy\tools\Arduino\arduino-cli.exe"
```

An exclusion reduces protection for that path. On a managed fleet, change the
rule to audit instead so the decision stays visible.

### Smart App Control (Windows 11)

SAC has **no per-app allowlist**. A self-signed or internally-issued certificate
does not satisfy it; only reputation from a publicly trusted certificate does.
The practical options are turning SAC off (irreversible without reinstalling
Windows) or compiling on the server instead of the workstation.

## Signing the toolchain

Signing must happen **at package time, before the archive is built** — the app
replaces the whole `Arduino/` tree when it refreshes the toolchain, so signatures
applied on an end-user machine are lost at the next update.

```powershell
$signtool = "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.0.19041.0\x64\signtool.exe"
Get-ChildItem 'tools\Arduino' -Recurse -Filter *.exe | ForEach-Object {
  & $signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 `
    /sha1 <THUMBPRINT> $_.FullName
}
```

Requirements:

- Sign **all 140 executables**, not just `arduino-cli`. Missing the compiler only
  moves the block to `xtensa-esp32s3-elf-g++.exe`.
- Always timestamp (`/tr`): without it every signature dies when the certificate
  expires and the machines break again.
- Sign DLLs too if the WDAC policy enables DLL rules (off by default).
- Rebuild the tools archive (`shell/scripts/archive-tools.sh`) after signing and
  update any published checksums.
- Redistributing re-signed third-party binaries is permitted for Apache-2.0
  `arduino-cli` and the Espressif toolchain, but keep their notices intact.

## What the app does now

- **Startup preflight** — the app runs `arduino-cli version` once, and when the
  OS refuses to start it, logs the cause and raises a toast instead of failing
  silently later.
- **Signature probe** — `toolchain::warn_if_unsigned` reads the PE certificate
  table and warns at startup when the launcher has no signature at all.
- **Actionable build/flash errors** — a launch refusal on either the normal or
  the elevated (elevated goes through `cmd.exe`, so the refusal arrives as log
  text) path is mapped to a message naming Application Control and this document.

Implementation: `shell/src/toolchain.rs` (classification, PE parser, probe) and
`shell/src/upload/arduino.rs` (build and flash error mapping).

## Verify

```powershell
& 'C:\futureacademy\tools\Arduino\arduino-cli.exe' version
& 'C:\futureacademy\tools\Arduino\packages\esp32\tools\esp-x32\*\bin\xtensa-esp32s3-elf-g++.exe' --version
Get-WinEvent -LogName 'Microsoft-Windows-CodeIntegrity/Operational' -MaxEvents 10 |
  Where-Object { $_.Message -match 'arduino|xtensa' }   # expect no new events
```
