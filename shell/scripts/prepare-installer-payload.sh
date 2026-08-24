#!/bin/bash
# Prepare installer payload (tools archive, executables, firmwares)
# Usage: ./prepare-installer-payload.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_ROOT/.." && pwd)"

PAYLOAD_ROOT="$REPO_ROOT/dist/installer-payload"
TOOLS_ROOT="$REPO_ROOT/tools"
FIRMWARES_ROOT="$REPO_ROOT/firmwares"
ASSETS_ROOT="$REPO_ROOT/installer/assets"

# Version from Cargo.toml
VERSION="$(grep '^version = ' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\([^"]*\)".*/\1/')"

echo "[prepare-installer-payload] version=$VERSION"

# Create payload directory
mkdir -p "$PAYLOAD_ROOT"

# Check tools directory
if [[ ! -d "$TOOLS_ROOT" ]]; then
    echo "Error: Tools directory not found: $TOOLS_ROOT"
    echo "Run ./scripts/download-tools.sh first."
    exit 1
fi

# Check firmwares directory
if [[ ! -d "$FIRMWARES_ROOT" ]]; then
    echo "Error: Firmwares directory not found: $FIRMWARES_ROOT"
    exit 1
fi

# Create tools.7z
echo "Creating tools.7z..."
TOOLS_ARCHIVE="$PAYLOAD_ROOT/tools.7z"
if command -v 7z &> /dev/null; then
    7z a -t7z -mx=9 -sccUTF-8 "$TOOLS_ARCHIVE" tools/
elif command -v 7za &> /dev/null; then
    7za a -t7z -mx=9 -sccUTF-8 "$TOOLS_ARCHIVE" tools/
else
    echo "Error: 7-Zip not found."
    exit 1
fi

# Copy 7za.exe
if [[ -f "$PROJECT_ROOT/7za.exe" ]]; then
    cp "$PROJECT_ROOT/7za.exe" "$PAYLOAD_ROOT/"
elif [[ -f "$PROJECT_ROOT/7zr.exe" ]]; then
    cp "$PROJECT_ROOT/7zr.exe" "$PAYLOAD_ROOT/"
fi

# Copy tray executable
TRAY_EXE="$PROJECT_ROOT/target/x86_64-pc-windows-gnu/release/FutureAcademyTray.exe"
if [[ -f "$TRAY_EXE" ]]; then
    cp "$TRAY_EXE" "$PAYLOAD_ROOT/FutureAcademyTray.exe"
else
    echo "Warning: Tray executable not found at expected path"
fi

# Copy firmwares
echo "Copying firmwares..."
cp -r "$FIRMWARES_ROOT" "$PAYLOAD_ROOT/"

# Write version file
echo "$VERSION" > "$PAYLOAD_ROOT/version.txt"

# Write build type
echo "cli" > "$PAYLOAD_ROOT/build-type.txt"

# Calculate total size
TOTAL_SIZE=$(du -sb "$PAYLOAD_ROOT" | cut -f1)

echo ""
echo "Installer payload ready: $PAYLOAD_ROOT"
echo "Total payload size: $(numfmt --to=iec $TOTAL_SIZE 2>/dev/null || echo "${TOTAL_SIZE}B")"
