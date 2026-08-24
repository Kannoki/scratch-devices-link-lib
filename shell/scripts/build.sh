#!/bin/bash
# Build script for Future Academy Link shell
# Usage: ./build.sh [--release] [--target TARGET]
# Example: ./build.sh --release
#          ./build.sh --release --target aarch64-apple-darwin

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Detect cargo binary
detect_cargo() {
    if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]]; then
        if [[ -x "$HOME/.cargo/bin/cargo.exe" ]]; then
            echo "$HOME/.cargo/bin/cargo.exe"
        else
            echo "cargo"
        fi
    else
        if [[ -x "$HOME/.cargo/bin/cargo" ]]; then
            echo "$HOME/.cargo/bin/cargo"
        else
            echo "cargo"
        fi
    fi
}

CARGO="$(detect_cargo)"

# Parse arguments
MANIFEST_PATH="$PROJECT_ROOT/Cargo.toml"
TARGET=""
BUILD_ARGS=()

while [[ $# -gt 0 ]]; do
    case $1 in
        --release)
            BUILD_ARGS+=("--release")
            shift
            ;;
        --target)
            TARGET="$2"
            BUILD_ARGS+=("--target" "$2")
            shift 2
            ;;
        --manifest-path)
            MANIFEST_PATH="$2"
            shift 2
            ;;
        *)
            BUILD_ARGS+=("$1")
            shift
            ;;
    esac
done

echo "[build] Using cargo: $CARGO"
echo "[build] Manifest: $MANIFEST_PATH"
if [[ -n "$TARGET" ]]; then
    echo "[build] Target: $TARGET"
fi

# Run cargo build
"$CARGO" build --manifest-path "$MANIFEST_PATH" "${BUILD_ARGS[@]}"

echo "[build] Done"
