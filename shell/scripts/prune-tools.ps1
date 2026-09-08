# Prune unused tools from the tools directory on Windows
# Usage: .\prune-tools.ps1 [-Apply]

param(
    [switch]$Apply = $false
)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..\..")
$ToolsRoot = Join-Path $RepoRoot "tools"
$ArduinoRoot = Join-Path $ToolsRoot "Arduino"

if (-not (Test-Path $ArduinoRoot)) {
    Write-Error "Tools directory not found: $ArduinoRoot"
    exit 1
}

$action = if ($Apply) { "Removing" } else { "Would remove" }
Write-Host "[prune-tools] $action unused tools" -ForegroundColor Cyan

$RemovePaths = @(
    "Python",
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
    "Arduino\packages\esp32\tools\esp-x32\2405\xtensa-esp-elf\lib\esp32s2"
)

# Removable non-S3 ESP32 Arduino libs
$esp32LibsRoot = Join-Path $ToolsRoot "Arduino\packages\esp32\tools\esp32-arduino-libs"
if (Test-Path $esp32LibsRoot) {
    Get-ChildItem -Path $esp32LibsRoot -Directory | ForEach-Object {
        $verDir = $_.FullName
        $verName = $_.Name
        foreach ($chip in @("esp32", "esp32c3", "esp32c6", "esp32h2", "esp32p4", "esp32s2")) {
            $chipPath = Join-Path $verDir $chip
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

$existingRemove = @()
[long]$totalBytes = 0

foreach ($relPath in $RemovePaths) {
    $fullPath = Join-Path $ToolsRoot $relPath
    if (Test-Path $fullPath) {
        $item = Get-Item $fullPath
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

foreach ($entry in $existingRemove) {
    if (Test-Path $entry.FullPath) {
        Remove-Item -LiteralPath $entry.FullPath -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Truncate library_index.json to minimal valid schema to prevent CLI from re-downloading 54MB
$libIndex = Join-Path $ArduinoRoot "library_index.json"
if (Test-Path $libIndex) {
    Set-Content -Path $libIndex -Value '{"libraries":[]}' -Force
}

Write-Host ""
Write-Host "Pruned tools. Removed approximately $totalMB MB." -ForegroundColor Green
