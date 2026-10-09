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

IS_RELEASE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --release)
            IS_RELEASE=true
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

# Determine build profile directory
if [[ "$IS_RELEASE" == true ]]; then
    PROFILE="release"
else
    PROFILE="debug"
fi

# Locate compiled executable
if [[ -n "$TARGET" ]]; then
    TARGET_DIR="$PROJECT_ROOT/target/$TARGET/$PROFILE"
else
    TARGET_DIR="$PROJECT_ROOT/target/$PROFILE"
fi

BIN_PATH=""
for bin_name in "FutureAcademy.exe" "FutureAcademy" "FutureAcademyTray.exe" "FutureAcademyTray"; do
    if [[ -f "$TARGET_DIR/$bin_name" ]]; then
        BIN_PATH="$TARGET_DIR/$bin_name"
        break
    fi
done

REPO_ROOT="$(cd "$PROJECT_ROOT/.." && pwd)"

if [[ -z "$BIN_PATH" ]]; then
    echo "Warning: Compiled binary not found in $TARGET_DIR"
else
    VERSION="$(grep '^version = ' "$MANIFEST_PATH" | head -1 | sed 's/.*"\([^"]*\)".*/\1/')"
    if [[ -z "$VERSION" ]]; then
        VERSION="dev"
    fi

    DIST_DIR="$REPO_ROOT/dist/FutureAcademy-${VERSION}"
    mkdir -p "$DIST_DIR"

    DEST_BIN_NAME="$(basename "$BIN_PATH")"
    if [[ "$DEST_BIN_NAME" == "FutureAcademyTray.exe" ]]; then
        DEST_BIN_NAME="FutureAcademy.exe"
    elif [[ "$DEST_BIN_NAME" == "FutureAcademyTray" ]]; then
        DEST_BIN_NAME="FutureAcademy"
    fi

    cp "$BIN_PATH" "$DIST_DIR/$DEST_BIN_NAME"
    echo "$VERSION" > "$DIST_DIR/version.txt"
    echo "[build] Copied binary to $DIST_DIR/$DEST_BIN_NAME"
fi

echo "[build] Done"
