#!/bin/bash
# Download tools (arduino-cli + ESP32 toolchain) from GitHub releases
# Usage: ./download-tools.sh [--platform win32|darwin|linux] [--extract-path PATH]
# Defaults: Windows → C:\futureacademy\tools, macOS/Linux → ./tools

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_ROOT/.." && pwd)"

# Configuration
WINBLOCK_TOOLS_USER="${WINBLOCK_TOOLS_USER:-winblockcc}"
WINBLOCK_TOOLS_REPO="${WINBLOCK_TOOLS_REPO:-winblock-tools}"
WINBLOCK_TOOLS_TAG="${WINBLOCK_TOOLS_TAG:-}"

# Parse arguments
PLATFORM="$(uname -s | tr '[:upper:]' '[:lower:]')"
EXTRACT_PATH=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --platform)
            PLATFORM="$2"
            shift 2
            ;;
        --extract-path)
            EXTRACT_PATH="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done

# Set default extract path
if [[ -z "$EXTRACT_PATH" ]]; then
    if [[ "$PLATFORM" == "win32" ]]; then
        EXTRACT_PATH="C:\\futureacademy\\tools"
    else
        EXTRACT_PATH="$REPO_ROOT/tools"
    fi
fi

DOWNLOAD_PATH="$REPO_ROOT/tmp"
RELEASE_API_URL=""

if [[ -n "$WINBLOCK_TOOLS_TAG" ]]; then
    RELEASE_API_URL="https://api.github.com/repos/${WINBLOCK_TOOLS_USER}/${WINBLOCK_TOOLS_REPO}/releases/tags/${WINBLOCK_TOOLS_TAG}"
else
    RELEASE_API_URL="https://api.github.com/repos/${WINBLOCK_TOOLS_USER}/${WINBLOCK_TOOLS_REPO}/releases/latest"
fi

echo "[download-tools] platform=$PLATFORM extract=$EXTRACT_PATH"

# Create directories
mkdir -p "$DOWNLOAD_PATH"
mkdir -p "$EXTRACT_PATH"

# Detect download tool
if command -v curl &> /dev/null; then
    DOWNLOAD_CMD="curl -fsSL"
elif command -v wget &> /dev/null; then
    DOWNLOAD_CMD="wget -qO-"
else
    echo "Error: Neither curl nor wget found"
    exit 1
fi

# Fetch release data
echo "Fetching release info..."
RELEASE_DATA="$($DOWNLOAD_CMD "$RELEASE_API_URL")" || {
    echo "Error: Failed to fetch release from GitHub"
    echo "Set WINBLOCK_TOOLS_USER/WINBLOCK_TOOLS_REPO/WINBLOCK_TOOLS_TAG to override"
    exit 1
}

# Extract asset URLs using grep/sed (POSIX-compatible JSON parsing)
# Find .7z assets for the target platform
case "$PLATFORM" in
    win32|cygwin|msys)
        ASSET_PATTERN="win"
        ;;
    darwin)
        ASSET_PATTERN="darwin|macos|mac"
        ;;
    linux)
        ASSET_PATTERN="linux"
        ;;
    *)
        ASSET_PATTERN="$PLATFORM"
        ;;
esac

echo "Looking for assets matching: $ASSET_PATTERN"

# Download and extract each matching asset
# Note: This is a simplified version. Full implementation would need proper JSON parsing.

echo "[download-tools] Note: For full toolchain download, use the pre-built .7z archives from GitHub releases"
echo "[download-tools] Release API: $RELEASE_API_URL"

# Check if 7z is available for extraction
if command -v 7z &> /dev/null; then
    EXTRACT_CMD="7z x -o"
elif command -v 7za &> /dev/null; then
    EXTRACT_CMD="7za x -o"
else
    echo "Warning: 7-Zip not found. Install 7z or 7za for extraction support."
    EXTRACT_CMD=""
fi

echo "[download-tools] Done"
