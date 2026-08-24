#!/bin/bash
# Build Inno Setup installer for Windows
# Usage: ./build-setup.sh [wizard|standard]
#   wizard    - Build wizard-style installer (default)
#   standard  - Build standard installer

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_ROOT/.." && pwd)"

PAYLOAD_ROOT="$REPO_ROOT/dist/installer-payload"

# Version from Cargo.toml
VERSION="$(grep '^version = ' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\([^"]*\)".*/\1/')"

echo "[build-setup] version=$VERSION"

# Determine which ISS file to build
INSTALLER_TYPE="${1:-wizard}"
case "$INSTALLER_TYPE" in
    wizard)
        ISS_PATH="$REPO_ROOT/installer/FutureAcademyLinkWizard.iss"
        OUTPUT_NAME="FutureAcademy-${VERSION}-x64-wizard-setup"
        ;;
    standard)
        ISS_PATH="$REPO_ROOT/installer/FutureAcademyLink.iss"
        OUTPUT_NAME="FutureAcademy-${VERSION}-x64-setup"
        ;;
    *)
        echo "Error: Unknown installer type '$INSTALLER_TYPE'"
        echo "Usage: $0 [wizard|standard]"
        exit 1
        ;;
esac

SETUP_OUT="$REPO_ROOT/dist/${OUTPUT_NAME}.exe"
echo "[build-setup] type=$INSTALLER_TYPE"

# Check payload
if [[ ! -d "$PAYLOAD_ROOT" ]]; then
    echo "Error: Installer payload not found: $PAYLOAD_ROOT"
    echo "Run ./scripts/prepare-installer-payload.sh first."
    exit 1
fi

# Check Inno Setup
ISCC=""
if command -v ISCC &> /dev/null; then
    ISCC="ISCC"
elif [[ -x "/c/Program Files (x86)/Inno Setup 6/ISCC.exe" ]]; then
    ISCC="/c/Program Files (x86)/Inno Setup 6/ISCC.exe"
elif [[ -x "/c/Program Files/Inno Setup 6/ISCC.exe" ]]; then
    ISCC="/c/Program Files/Inno Setup 6/ISCC.exe"
fi

if [[ -z "$ISCC" ]]; then
    echo "Error: Inno Setup 6 (ISCC.exe) not found."
    echo "Install with: winget install JRSoftware.InnoSetup"
    exit 1
fi

# Run Inno Setup
echo "Building installer..."
"$ISCC" "$ISS_PATH" "/DAppVersion=$VERSION" "/DOutputBaseFilename=$OUTPUT_NAME"

if [[ ! -f "$SETUP_OUT" ]]; then
    echo "Error: Expected output was not created: $SETUP_OUT"
    exit 1
fi

SIZE=$(du -h "$SETUP_OUT" | cut -f1)
echo "Setup created: $SETUP_OUT ($SIZE)"

# Optional code signing
if [[ -n "$WIN_SIGN_PFX_PATH" ]]; then
    echo "Signing installer..."
    ./sign.sh "$SETUP_OUT"
else
    echo "Warning: Installer is unsigned - Windows SmartScreen may block it."
    echo "Set WIN_SIGN_PFX_PATH (+ WIN_SIGN_PFX_PASSWORD) to sign."
fi
