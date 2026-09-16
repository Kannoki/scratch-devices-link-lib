# Prune unused tools from the tools directory
# Usage: .\prune-tools.ps1 [-Apply] [-ToolsPath <path>]

param(
    [switch]$Apply = $false,
    [string]$ToolsPath = ""
)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..\..")
$ToolsRoot = if ($ToolsPath) { Resolve-Path $ToolsPath } else { Join-Path $RepoRoot "tools" }
$ArduinoRoot = Join-Path $ToolsRoot "Arduino"

if (-not (Test-Path $ArduinoRoot)) {
    Write-Error "Arduino tools directory not found under: $ToolsRoot"
    exit 1
}

$action = if ($Apply) { "Removing" } else { "Would remove" }
Write-Host "[prune-tools] $action unused tools from: $ToolsRoot" -ForegroundColor Cyan

$RemovePaths = @(
    "Python",
    ".DS_Store",
    "Arduino\.DS_Store",
    "Arduino\.vscode",
    "Arduino\staging\libraries",
    "Arduino\package_rp2040_index.json",
    "Arduino\package_esp8266com_index.json",
    "Arduino\package_Maixduino_k210_dl_cdn_index.json",
    "Arduino\package_Maixduino_k210_index.json",
    "Arduino\package_sparkfun_index.json",
    "Arduino\packages\Maixduino",
    "Arduino\packages\SparkFun",
    "Arduino\packages\esp8266",
    "Arduino\packages\rp2040",
    "Arduino\packages\builtin\tools\dfu-discovery",
    "Arduino\packages\builtin\tools\mdns-discovery",
    "Arduino\packages\builtin\tools\serial-discovery\1.2.1",
    "Arduino\packages\builtin\tools\serial-discovery\1.3.2",
    "Arduino\packages\builtin\tools\serial-monitor",
    "Arduino\packages\arduino\hardware\renesas_uno",
    "Arduino\packages\arduino\tools\arduinoOTA",
    "Arduino\packages\arduino\tools\arm-none-eabi-gcc",
    "Arduino\packages\arduino\tools\bossac",
    "Arduino\packages\arduino\tools\dfu-util",
    "Arduino\packages\arduino\tools\openocd",
    "Arduino\packages\esp32\tools\esp-rv32",
    "Arduino\packages\esp32\tools\openocd-esp32",
    "Arduino\packages\esp32\tools\riscv32-esp-elf-gcc",
    "Arduino\packages\esp32\tools\riscv32-esp-elf-gdb",
    "Arduino\packages\esp32\tools\xtensa-esp-elf-gdb",
    "Arduino\packages\esp32\tools\xtensa-esp32-elf-gcc",
    "Arduino\packages\esp32\tools\xtensa-esp32s2-elf-gcc",
    "Arduino\packages\esp32\tools\xtensa-esp32s3-elf-gcc",
    "Arduino\packages\esp32\tools\esp-x32\2405\xtensa-esp-elf\lib\esp32",
    "Arduino\packages\esp32\tools\esp-x32\2405\xtensa-esp-elf\lib\esp32s2",
    "Arduino\packages\esp32\tools\esp-x32\2405\lib\gcc\xtensa-esp-elf\13.2.0\esp32",
    "Arduino\packages\esp32\tools\esp-x32\2405\lib\gcc\xtensa-esp-elf\13.2.0\esp32s2",
    "Arduino\packages\esp32\tools\esp-x32\2405\lib\xtensa_esp32.so",
    "Arduino\packages\esp32\tools\esp-x32\2405\lib\xtensa_esp32s2.so",
    "Arduino\packages\esp32\tools\esp-x32\2405\lib\xtensa_esp8266.so"
)

# AVR unneeded drivers & firmwares
$avrHardwareRoot = Join-Path $ToolsRoot "Arduino\packages\arduino\hardware\avr"
if (Test-Path $avrHardwareRoot) {
    Get-ChildItem -Path $avrHardwareRoot -Directory | ForEach-Object {
        $ver = $_.Name
        foreach ($folder in @("drivers", "firmwares")) {
            $p = Join-Path $avrHardwareRoot "$ver\$folder"
            if (Test-Path $p) {
                $RemovePaths += "Arduino\packages\arduino\hardware\avr\$ver\$folder"
            }
        }
    }
}

# Removable non-S3 ESP32 Arduino libs
$esp32LibsRoot = Join-Path $ToolsRoot "Arduino\packages\esp32\tools\esp32-arduino-libs"
if (Test-Path $esp32LibsRoot) {
    Get-ChildItem -Path $esp32LibsRoot -Directory | ForEach-Object {
        $verName = $_.Name
        foreach ($chip in @("esp32", "esp32c3", "esp32c6", "esp32h2", "esp32p4", "esp32s2")) {
            $chipPath = Join-Path $_.FullName $chip
            if (Test-Path $chipPath) {
                $RemovePaths += "Arduino\packages\esp32\tools\esp32-arduino-libs\$verName\$chip"
            }
        }
    }
}

# Non-S3 wrapper binaries in esp-x32
$binRoot = Join-Path $ToolsRoot "Arduino\packages\esp32\tools\esp-x32\2405\bin"
if (Test-Path $binRoot) {
    Get-ChildItem -Path $binRoot -File | Where-Object {
        $_.Name -like "xtensa-esp32-elf-*" -or $_.Name -like "xtensa-esp32s2-elf-*"
    } | ForEach-Object {
        $RemovePaths += "Arduino\packages\esp32\tools\esp-x32\2405\bin\$($_.Name)"
    }
}

# Build artifacts & git folders in libraries (.pio, .vscode, .git, .github)
$librariesRoot = Join-Path $ToolsRoot "Arduino\libraries"
if (Test-Path $librariesRoot) {
    Get-ChildItem -Path $librariesRoot -Directory -Recurse -Force | Where-Object {
        $_.Name -in @(".pio", ".vscode", ".git", ".github")
    } | ForEach-Object {
        $rel = $_.FullName.Substring($ToolsRoot.ToString().Length).TrimStart("\/")
        $RemovePaths += $rel
    }

    # Library examples, tests, and documentation
    Get-ChildItem -Path $librariesRoot -Directory -Recurse | Where-Object {
        $_.Name -in @("examples", "example", "tests", "extras", "doc", "docs")
    } | ForEach-Object {
        $rel = $_.FullName.Substring($ToolsRoot.ToString().Length).TrimStart("\/")
        $RemovePaths += $rel
    }
}

# De-duplicate paths
$RemovePaths = $RemovePaths | Select-Object -Unique

$existingRemove = @()
[long]$totalBytes = 0

foreach ($relPath in $RemovePaths) {
    $fullPath = Join-Path $ToolsRoot $relPath
    if (Test-Path $fullPath) {
        $item = Get-Item $fullPath -Force
        $size = 0
        if ($item.PSIsContainer) {
            $size = (Get-ChildItem -LiteralPath $fullPath -Recurse -File -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
        } else {
            $size = $item.Length
        }
        if (-not $size) { $size = 0 }
        $totalBytes += $size
        $sizeMB = [math]::Round($size / 1MB, 2)
        $existingRemove += [PSCustomObject]@{
            RelPath = $relPath
            FullPath = $fullPath
            SizeMB = $sizeMB
        }
        Write-Host "  - tools\$relPath ($sizeMB MB)" -ForegroundColor Gray
    }
}

$totalMB = [math]::Round($totalBytes / 1MB, 2)
Write-Host ""
Write-Host "Total: $($existingRemove.Count) targets (~$totalMB MB)" -ForegroundColor Yellow

if (-not $Apply) {
    Write-Host ""
    Write-Host "Dry run only. Run with -Apply to prune tools." -ForegroundColor Green
    exit 0
}

Write-Host "Deleting $($existingRemove.Count) items..." -ForegroundColor Cyan
foreach ($entry in $existingRemove) {
    if (Test-Path $entry.FullPath) {
        Remove-Item -LiteralPath $entry.FullPath -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Remove any remaining .DS_Store files
Get-ChildItem -Path $ToolsRoot -Filter ".DS_Store" -Recurse -Force -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue

# Truncate library_index.json to minimal valid schema to prevent CLI from re-downloading 54MB
$libIndex = Join-Path $ArduinoRoot "library_index.json"
if (Test-Path $libIndex) {
    Set-Content -Path $libIndex -Value '{"libraries":[]}' -Force
}

Write-Host ""
Write-Host "Pruned tools. Removed approximately $totalMB MB." -ForegroundColor Green
