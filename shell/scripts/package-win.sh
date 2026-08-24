#!/bin/bash
# Package Future Academy Link for Windows distribution
# Usage: ./package-win.sh [--target TARGET]
# Creates dist/FutureAcademy-win/ with portable executable

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_ROOT/.." && pwd)"

# Parse arguments
TARGET="${1:-x86_64-pc-windows-gnu}"
TRAY_EXE="$PROJECT_ROOT/target/$TARGET/release/FutureAcademyTray.exe"

# Version from Cargo.toml
VERSION="$(grep '^version = ' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\([^"]*\)".*/\1/')"

# Output directories
DIST_DIR="$REPO_ROOT/dist"
OUT_DIR="$DIST_DIR/FutureAcademy-win"

echo "[package-win] target=$TARGET"
echo "[package-win] version=$VERSION"

# Check if shell binary exists
if [[ ! -f "$TRAY_EXE" ]]; then
    echo "Error: Shell binary not found: $TRAY_EXE"
    echo "Run ./scripts/build.sh --release --target $TARGET first."
    exit 1
fi

# Create output directory
mkdir -p "$OUT_DIR"

# Copy executable
echo "Copying FutureAcademyTray.exe..."
cp "$TRAY_EXE" "$OUT_DIR/"

# Write version file
echo "$VERSION" > "$OUT_DIR/version.txt"

# Write README
cat > "$OUT_DIR/README.txt" << 'README'
Future Academy Link — Windows

Run FutureAcademyTray.exe.
arduino-cli and esp32 core are bundled in tools/ next to this folder.

Editor: https://stem.windify.edu.vn/
README

# Copy tools directory if it exists
if [[ -d "$REPO_ROOT/tools" ]]; then
    echo "Bundling tools..."
    cp -r "$REPO_ROOT/tools" "$OUT_DIR/"
    echo "Bundled $(find "$OUT_DIR/tools" -type f | wc -l) files from tools/"
else
    echo "Warning: tools/ not found at repo root"
    echo "Run ./scripts/download-tools.sh first."
fi

# Copy 7zr.exe if it exists
if [[ -f "$PROJECT_ROOT/7zr.exe" ]]; then
    cp "$PROJECT_ROOT/7zr.exe" "$OUT_DIR/"
    echo "Bundled 7zr.exe for runtime extraction"
else
    echo "Warning: 7zr.exe not found — app will need 7-Zip installed"
fi

# Report size
SIZE_KB=$(du -k "$TRAY_EXE" | cut -f1)
SIZE_MB=$(echo "scale=1; $SIZE_KB / 1024" | bc 2>/dev/null || echo "N/A")

echo ""
echo "Built: $OUT_DIR"
echo "Size:  $SIZE_MB MB (FutureAcademyTray.exe)"
echo ""
echo "Tools (arduino-cli + esp32 core) are bundled in tools/ next to the binary."
