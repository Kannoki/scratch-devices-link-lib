#!/bin/bash
# Sign Windows artifacts with Authenticode
# Usage: ./sign.sh <file1> [file2] ...
# Requires: WIN_SIGN_PFX_PATH (and optionally WIN_SIGN_PFX_PASSWORD)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PFX_PATH="${WIN_SIGN_PFX_PATH:-}"
PFX_PASSWORD="${WIN_SIGN_PFX_PASSWORD:-}"
TIMESTAMP_URL="${WIN_SIGN_TIMESTAMP_URL:-http://timestamp.digicert.com}"

if [[ -z "$PFX_PATH" ]]; then
    echo "[sign] WIN_SIGN_PFX_PATH not set — skipping Authenticode signing."
    exit 0
fi

if [[ ! -f "$PFX_PATH" ]]; then
    echo "Error: PFX not found: $PFX_PATH"
    exit 1
fi

# Find signtool
SIGNTOOL=""
if command -v signtool &> /dev/null; then
    SIGNTOOL="signtool"
elif [[ -x "/c/Program Files (x86)/Windows Kits/10/bin/10.0.22621.0/x64/signtool.exe" ]]; then
    SIGNTOOL="/c/Program Files (x86)/Windows Kits/10/bin/10.0.22621.0/x64/signtool.exe"
elif [[ -x "/c/Program Files (x86)/Windows Kits/10/bin/10.0.22000.0/x64/signtool.exe" ]]; then
    SIGNTOOL="/c/Program Files (x86)/Windows Kits/10/bin/10.0.22000.0/x64/signtool.exe"
fi

if [[ -z "$SIGNTOOL" ]]; then
    echo "Error: signtool.exe not found. Install Windows SDK."
    exit 1
fi

# Sign each file
for FILE in "$@"; do
    if [[ ! -f "$FILE" ]]; then
        echo "Warning: Skip missing $FILE"
        continue
    fi

    echo "[sign] $(basename "$FILE")"

    ARGS=(
        sign
        /fd SHA256
        /tr "$TIMESTAMP_URL"
        /td SHA256
        /f "$PFX_PATH"
    )

    if [[ -n "$PFX_PASSWORD" ]]; then
        ARGS+=("/p" "$PFX_PASSWORD")
    fi

    ARGS+=("$FILE")

    "$SIGNTOOL" "${ARGS[@]}"
done

echo "[sign] Done"
