#!/bin/bash
# Extract tools from a 7z archive
# Usage: ./extract-7z.sh <path-to-tools.7z>

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_ROOT/.." && pwd)"

ARCHIVE_PATH="${1:-}"
EXTRACT_PATH="$REPO_ROOT/tools"

if [[ -z "$ARCHIVE_PATH" ]]; then
    echo "Usage: $0 <path-to-tools.7z>"
    exit 1
fi

if [[ ! -f "$ARCHIVE_PATH" ]]; then
    echo "Error: Archive not found: $ARCHIVE_PATH"
    exit 1
fi

echo "Extracting $ARCHIVE_PATH → $EXTRACT_PATH"

# Remove existing tools directory
if [[ -d "$EXTRACT_PATH" ]]; then
    rm -rf "$EXTRACT_PATH"
fi

# Create output directory
mkdir -p "$EXTRACT_PATH"

# Extract using 7z
if command -v 7z &> /dev/null; then
    7z x -o"$EXTRACT_PATH" "$ARCHIVE_PATH"
elif command -v 7za &> /dev/null; then
    7za x -o"$EXTRACT_PATH" "$ARCHIVE_PATH"
else
    echo "Error: 7-Zip not found. Install p7zip or 7z."
    exit 1
fi

echo "Extract complete."
