#!/bin/bash
# Archive tools directory for distribution
# Usage: ./archive-tools.sh [--overwrite]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_ROOT/.." && pwd)"

TOOLS_ROOT="$REPO_ROOT/tools"
OUTPUT_DIR="$REPO_ROOT/tmp"
ARCHIVE_NAME="tools-pruned-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m).7z"
ARCHIVE_PATH="$OUTPUT_DIR/$ARCHIVE_NAME"

OVERWRITE=false
if [[ "$1" == "--overwrite" ]]; then
    OVERWRITE=true
fi

echo "[archive-tools] Creating $ARCHIVE_NAME"

# Check if tools exist
if [[ ! -d "$TOOLS_ROOT" ]]; then
    echo "Error: Tools directory not found: $TOOLS_ROOT"
    echo "Run ./scripts/download-tools.sh first."
    exit 1
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check if archive exists
if [[ -f "$ARCHIVE_PATH" && "$OVERWRITE" == "false" ]]; then
    echo "Error: Archive already exists: $ARCHIVE_PATH"
    echo "Use --overwrite to replace it."
    exit 1
fi

# Remove existing archive if overwriting
if [[ -f "$ARCHIVE_PATH" ]]; then
    rm -f "$ARCHIVE_PATH"
fi

# Create archive using 7z
if command -v 7z &> /dev/null; then
    7z a -t7z -mx=9 -sccUTF-8 "$ARCHIVE_PATH" tools/
elif command -v 7za &> /dev/null; then
    7za a -t7z -mx=9 -sccUTF-8 "$ARCHIVE_PATH" tools/
else
    echo "Error: 7-Zip not found. Install p7zip or 7z."
    exit 1
fi

SIZE=$(du -h "$ARCHIVE_PATH" | cut -f1)
echo "Archive created: $ARCHIVE_PATH ($SIZE)"
