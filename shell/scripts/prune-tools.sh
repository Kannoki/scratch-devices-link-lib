#!/bin/bash
# Prune unused tools from the tools directory
# Usage: ./prune-tools.sh [--apply] [--tools-path <path>]
# Without --apply, shows what would be removed (dry run)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_ROOT/.." && pwd)"

TOOLS_ROOT="$REPO_ROOT/tools"
APPLY=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --apply)
            APPLY=true
            shift
            ;;
        --tools-path)
            TOOLS_ROOT="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done

ARDUINO_ROOT="$TOOLS_ROOT/Arduino"

echo "[prune-tools] $([ "$APPLY" == "true" ] && echo "Removing" || echo "Would remove") unused tools from: $TOOLS_ROOT"

# Check if tools directory exists
if [[ ! -d "$ARDUINO_ROOT" ]]; then
    echo "Error: Tools directory not found: $ARDUINO_ROOT"
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

# Base paths to remove
REMOVE_PATHS=(
    "Python"
    ".DS_Store"
    "Arduino/.DS_Store"
    "Arduino/.vscode"
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
    "Arduino/packages/builtin/tools/serial-discovery/1.2.1"
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
    "Arduino/packages/esp32/tools/esp-x32/2405/lib/gcc/xtensa-esp-elf/13.2.0/esp32"
    "Arduino/packages/esp32/tools/esp-x32/2405/lib/gcc/xtensa-esp-elf/13.2.0/esp32s2"
    "Arduino/packages/esp32/tools/esp-x32/2405/lib/xtensa_esp32.so"
    "Arduino/packages/esp32/tools/esp-x32/2405/lib/xtensa_esp32s2.so"
    "Arduino/packages/esp32/tools/esp-x32/2405/lib/xtensa_esp8266.so"
)

# AVR unneeded drivers & firmwares
if [[ -d "$TOOLS_ROOT/Arduino/packages/arduino/hardware/avr" ]]; then
    for ver_dir in "$TOOLS_ROOT/Arduino/packages/arduino/hardware/avr"/*/; do
        for folder in drivers firmwares; do
            if [[ -d "${ver_dir}${folder}" ]]; then
                REL_PATH="Arduino/packages/arduino/hardware/avr/$(basename "$ver_dir")/$folder"
                REMOVE_PATHS+=("$REL_PATH")
            fi
        done
    done
fi

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

# Find library build artifacts (.pio, .vscode, .git, .github) and examples/tests/docs
if [[ -d "$TOOLS_ROOT/Arduino/libraries" ]]; then
    while IFS= read -r -d '' dir; do
        REL_PATH="${dir#$TOOLS_ROOT/}"
        REMOVE_PATHS+=("$REL_PATH")
    done < <(find "$TOOLS_ROOT/Arduino/libraries" -type d \( -name ".pio" -o -name ".vscode" -o -name ".git" -o -name ".github" -o -name "examples" -o -name "example" -o -name "tests" -o -name "extras" -o -name "docs" -o -name "doc" \) -print0 2>/dev/null)
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

# Clean any remaining .DS_Store files
find "$TOOLS_ROOT" -name ".DS_Store" -delete 2>/dev/null || true

# Truncate library_index.json to minimal valid schema to prevent CLI from re-downloading 54MB
if [[ -f "$TOOLS_ROOT/Arduino/library_index.json" ]]; then
    echo '{"libraries":[]}' > "$TOOLS_ROOT/Arduino/library_index.json"
fi

echo ""
echo "Pruned tools. Removed approximately $(numfmt --to=iec $TOTAL_SIZE 2>/dev/null || echo "${TOTAL_SIZE}B")."
