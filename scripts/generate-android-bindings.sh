#!/usr/bin/env bash
#
# Generate UniFFI Kotlin bindings for the Android app.
#
# Prerequisites:
#   - Rust toolchain (cargo)
#   - uniffi-bindgen-cli 0.28.3:
#       cargo install uniffi-bindgen-cli@0.28.3
#
# Usage:
#   ./scripts/generate-android-bindings.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

UDL_FILE="$ROOT_DIR/bindings/uniffi/src/zvault.udl"
OUT_DIR="$ROOT_DIR/apps/android/app/src/main/java/com/zvault/uniffi"

echo "Building zvault-uniffi native library..."
cargo build -p zvault-uniffi

# Determine the library file path (Linux .so, macOS .dylib)
if [[ "$(uname -s)" == "Darwin" ]]; then
    LIB_FILE="$ROOT_DIR/target/debug/libzvault_uniffi.dylib"
else
    LIB_FILE="$ROOT_DIR/target/debug/libzvault_uniffi.so"
fi

if [[ ! -f "$LIB_FILE" ]]; then
    echo "ERROR: Library not found at $LIB_FILE"
    echo "       Make sure 'cargo build -p zvault-uniffi' succeeded."
    exit 1
fi

echo "Generating Kotlin bindings..."
uniffi-bindgen generate \
    "$UDL_FILE" \
    --language kotlin \
    --out-dir "$OUT_DIR" \
    --lib-file "$LIB_FILE"

echo "Done! Bindings written to: $OUT_DIR"
echo ""
echo "Generated files:"
ls -la "$OUT_DIR"/*.kt 2>/dev/null || echo "  (no .kt files found — check for errors above)"
