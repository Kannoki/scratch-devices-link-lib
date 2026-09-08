#!/bin/bash
# Prune unused tools from the tools directory
# Usage: ./prune-tools.sh [--apply]
# Without --apply, shows what would be removed (dry run)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_ROOT/.." && pwd)"

TOOLS_ROOT="$REPO_ROOT/tools"
ARDUINO_ROOT="$TOOLS_ROOT/Arduino"

APPLY=false
if [[ "$1" == "--apply" ]]; then
    APPLY=true
fi

echo "[prune-tools] $([ "$APPLY" == "true" ] && echo "Removing" || echo "Would remove") unused tools"

# Check if tools directory exists
if [[ ! -d "$ARDUINO_ROOT" ]]; then
    echo "Error: Tools directory not found: $ARDUINO_ROOT"
    echo "Run ./scripts/download-tools.sh first."
    exit 1
fi

# Required paths (keep these)
REQUIRED_PATHS=(
    "Arduino/arduino-cli"
    "Arduino/packages/arduino/hardware/avr"
    "Arduino/packages/arduino/tools/avr-gcc"
    "Arduino/packages/arduino/tools/avrdude"
    "Arduino/packages/builtin/tools/ctags"
    "Arduino/packages/esp32/hardware/esp32"
    "Arduino/packages/esp32/tools/esp-x32"
    "Arduino/packages/esp32/tools/esp32-arduino-libs"
    "Arduino/packages/esp32/tools/esptool_py"
)

# Paths to remove
REMOVE_PATHS=(
    "Python"
    "Arduino/staging/libraries"
    "Arduino/package_rp2040_index.json"
    "Arduino/package_esp8266com_index.json"
    "Arduino/package_Maixduino_k210_dl_cdn_index.json"
    "Arduino/package_Maixduino_k210_index.json"
    "Arduino/package_sparkfun_index.json"
    "Arduino/packages/Maixduino"
    "Arduino/packages/SparkFun"
    "Arduino/packages/esp8266"
    "Arduino/packages/rp2040"
    "Arduino/packages/builtin/tools/dfu-discovery"
    "Arduino/packages/builtin/tools/mdns-discovery"
    "Arduino/packages/builtin/tools/serial-discovery/1.3.2"
    "Arduino/packages/builtin/tools/serial-monitor"
    "Arduino/packages/arduino/hardware/renesas_uno"
    "Arduino/packages/arduino/tools/arduinoOTA"
    "Arduino/packages/arduino/tools/arm-none-eabi-gcc"
    "Arduino/packages/arduino/tools/bossac"
    "Arduino/packages/arduino/tools/dfu-util"
    "Arduino/packages/arduino/tools/openocd"
    "Arduino/packages/esp32/tools/esp-rv32"
    "Arduino/packages/esp32/tools/openocd-esp32"
    "Arduino/packages/esp32/tools/riscv32-esp-elf-gcc"
    "Arduino/packages/esp32/tools/riscv32-esp-elf-gdb"
    "Arduino/packages/esp32/tools/xtensa-esp-elf-gdb"
    "Arduino/packages/esp32/tools/xtensa-esp32-elf-gcc"
    "Arduino/packages/esp32/tools/xtensa-esp32s2-elf-gcc"
    "Arduino/packages/esp32/tools/xtensa-esp32s3-elf-gcc"
    "Arduino/packages/esp32/tools/esp-x32/2405/xtensa-esp-elf/lib/esp32"
    "Arduino/packages/esp32/tools/esp-x32/2405/xtensa-esp-elf/lib/esp32s2"
)

# Find removable ESP32 lib targets
if [[ -d "$TOOLS_ROOT/Arduino/packages/esp32/tools/esp32-arduino-libs" ]]; then
    for version_dir in "$TOOLS_ROOT/Arduino/packages/esp32/tools/esp32-arduino-libs"/*/; do
        for chip in esp32 esp32c3 esp32c6 esp32h2 esp32p4 esp32s2; do
            if [[ -d "${version_dir}${chip}" ]]; then
                REL_PATH="Arduino/packages/esp32/tools/esp32-arduino-libs/$(basename "$version_dir")/$chip"
                REMOVE_PATHS+=("$REL_PATH")
            fi
        done
    done
fi

# Find non-S3 bin wrappers in esp-x32
if [[ -d "$TOOLS_ROOT/Arduino/packages/esp32/tools/esp-x32/2405/bin" ]]; then
    for file in "$TOOLS_ROOT/Arduino/packages/esp32/tools/esp-x32/2405/bin"/xtensa-esp32-elf-* "$TOOLS_ROOT/Arduino/packages/esp32/tools/esp-x32/2405/bin"/xtensa-esp32s2-elf-*; do
        if [[ -f "$file" ]]; then
            REMOVE_PATHS+=("Arduino/packages/esp32/tools/esp-x32/2405/bin/$(basename "$file")")
        fi
    done
fi

# Filter to only existing paths
EXISTING_REMOVE=()
TOTAL_SIZE=0

for path in "${REMOVE_PATHS[@]}"; do
    FULL_PATH="$TOOLS_ROOT/$path"
    if [[ -e "$FULL_PATH" ]]; then
        SIZE=$(du -sb "$FULL_PATH" 2>/dev/null | cut -f1 || echo "0")
        TOTAL_SIZE=$((TOTAL_SIZE + SIZE))
        EXISTING_REMOVE+=("$path")
        echo "  - tools/$path ($(numfmt --to=iec $SIZE 2>/dev/null || echo "${SIZE}B"))"
    fi
done

echo ""
echo "Total: ${#EXISTING_REMOVE[@]} paths (~$(numfmt --to=iec $TOTAL_SIZE 2>/dev/null || echo "${TOTAL_SIZE}B"))"

if [[ "$APPLY" == "false" ]]; then
    echo ""
    echo "Dry run only. Re-run with --apply to prune tools."
    exit 0
fi

# Apply removal
for path in "${EXISTING_REMOVE[@]}"; do
    FULL_PATH="$TOOLS_ROOT/$path"
    rm -rf "$FULL_PATH"
done

# Truncate library_index.json to minimal valid schema to prevent CLI from re-downloading 54MB
if [[ -f "$TOOLS_ROOT/Arduino/library_index.json" ]]; then
    echo '{"libraries":[]}' > "$TOOLS_ROOT/Arduino/library_index.json"
fi

echo ""
echo "Pruned tools. Removed approximately $(numfmt --to=iec $TOTAL_SIZE 2>/dev/null || echo "${TOTAL_SIZE}B")."
