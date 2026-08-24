#!/bin/bash
# Clean distribution build outputs
# Usage: ./clean-dist.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_ROOT/.." && pwd)"

DIST_ROOT="$REPO_ROOT/dist"

echo "[clean-dist] Cleaning distribution outputs..."

if [[ ! -d "$DIST_ROOT" ]]; then
    echo "No dist directory found."
    exit 0
fi

# Remove installer payload
if [[ -d "$DIST_ROOT/installer-payload" ]]; then
    rm -rf "$DIST_ROOT/installer-payload"
    echo "  Removed dist/installer-payload"
fi

# Remove electron folders
for dir in "$DIST_ROOT"/electron "$DIST_ROOT"/electron-*; do
    if [[ -d "$dir" ]]; then
        rm -rf "$dir"
        echo "  Removed $(basename "$dir")"
    fi
done

# Remove stale renamed folders
for dir in "$DIST_ROOT"/*.stale-*; do
    if [[ -d "$dir" ]]; then
        rm -rf "$dir"
        echo "  Removed $(basename "$dir")"
    fi
done

# Remove electron output pointer
if [[ -f "$DIST_ROOT/electron-output.txt" ]]; then
    rm -f "$DIST_ROOT/electron-output.txt"
    echo "  Removed dist/electron-output.txt"
fi

echo "[clean-dist] Done"
