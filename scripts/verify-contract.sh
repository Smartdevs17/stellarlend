#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
STELLAR_LEND_DIR="${REPO_ROOT}/stellar-lend"

MODE="${1:-help}"

print_usage() {
    echo "Usage: $0 <command> [args]"
    echo ""
    echo "Commands:"
    echo "  build              Build all contracts and output WASM artifacts"
    echo "  hash               Print SHA256 hashes of built WASM files"
    echo "  diff <old> <new>   Diff two WASM files (old vs new)"
    echo "  verify <wasm>      Verify a WASM artifact exists and is valid"
    echo "  help               Show this help message"
}

build_contracts() {
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Building StellarLend Contracts"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    if ! command -v cargo &> /dev/null; then
        echo "ERROR: cargo not found. Install Rust toolchain: https://rustup.rs/"
        exit 1
    fi

    cd "${STELLAR_LEND_DIR}"

    echo "[1/2] Building all contracts..."
    cargo build --release --target wasm32-unknown-unknown 2>&1
    echo "      Build complete."

    echo "[2/2] Optimizing WASM artifacts..."
    if command -v wasm-opt &> /dev/null; then
        for wasm in target/wasm32-unknown-unknown/release/*.wasm; do
            if [ -f "$wasm" ] && [[ "$wasm" != *".d."* ]]; then
                wasm-opt -Oz "$wasm" -o "$wasm" 2>/dev/null || true
            fi
        done
        echo "      Optimization complete."
    else
        echo "      wasm-opt not found, skipping optimization."
    fi

    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Build complete."
    echo "═══════════════════════════════════════════════════════════════"
}

compute_hashes() {
    echo "═══════════════════════════════════════════════════════════════"
    echo "  WASM Artifact Hashes"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    WASM_DIR="${STELLAR_LEND_DIR}/target/wasm32-unknown-unknown/release"

    if [ ! -d "$WASM_DIR" ]; then
        echo "ERROR: WASM directory not found. Run 'build' first."
        exit 1
    fi

    echo "File | SHA256"
    echo "-----|-------"

    for wasm in "${WASM_DIR}"/*.wasm; do
        if [ -f "$wasm" ] && [[ "$wasm" != *".d."* ]]; then
            HASH=$(sha256sum "$wasm" | awk '{print $1}')
            NAME=$(basename "$wasm")
            echo "${NAME} | ${HASH}"
        fi
    done
}

verify_wasm() {
    local wasm_file="${1:-}"

    if [ -z "$wasm_file" ]; then
        echo "ERROR: No WASM file specified."
        echo "Usage: $0 verify <path-to-wasm>"
        exit 1
    fi

    echo "═══════════════════════════════════════════════════════════════"
    echo "  Verifying WASM Artifact"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    if [ ! -f "$wasm_file" ]; then
        echo "FAIL: File not found: ${wasm_file}"
        exit 1
    fi

    echo "[1/4] Checking file exists... OK"

    local filesize
    filesize=$(stat -f%z "$wasm_file" 2>/dev/null || stat -c%s "$wasm_file" 2>/dev/null || echo "0")

    if [ "$filesize" -eq 0 ]; then
        echo "FAIL: File is empty"
        exit 1
    fi
    echo "[2/4] Checking file size... OK (${filesize} bytes)"

    local magic
    magic=$(xxd -l 4 -p "$wasm_file" 2>/dev/null || echo "")
    if [ "$magic" = "0061736d" ]; then
        echo "[3/4] Checking WASM magic bytes... OK"
    else
        echo "WARN: WASM magic bytes not found (got: ${magic})"
    fi

    local hash
    hash=$(sha256sum "$wasm_file" | awk '{print $1}')
    echo "[4/4] Computing SHA256... ${hash}"

    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Verification complete."
    echo "═══════════════════════════════════════════════════════════════"
}

diff_wasm() {
    local old_wasm="${1:-}"
    local new_wasm="${2:-}"

    if [ -z "$old_wasm" ] || [ -z "$new_wasm" ]; then
        echo "ERROR: Two WASM files required for diff."
        echo "Usage: $0 diff <old.wasm> <new.wasm>"
        exit 1
    fi

    echo "═══════════════════════════════════════════════════════════════"
    echo "  WASM Bytecode Diff"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    for f in "$old_wasm" "$new_wasm"; do
        if [ ! -f "$f" ]; then
            echo "ERROR: File not found: ${f}"
            exit 1
        fi
    done

    local old_size new_size
    old_size=$(stat -f%z "$old_wasm" 2>/dev/null || stat -c%s "$old_wasm" 2>/dev/null || echo "0")
    new_size=$(stat -f%z "$new_wasm" 2>/dev/null || stat -c%s "$new_wasm" 2>/dev/null || echo "0")

    local old_hash new_hash
    old_hash=$(sha256sum "$old_wasm" | awk '{print $1}')
    new_hash=$(sha256sum "$new_wasm" | awk '{print $1}')

    echo "Property       | Old                | New"
    echo "---------------|--------------------|--------------------"
    echo "File           | $(basename "$old_wasm") | $(basename "$new_wasm")"
    echo "Size (bytes)   | ${old_size}                | ${new_size}"
    echo "SHA256         | ${old_hash} | ${new_hash}"
    echo ""

    if [ "$old_hash" = "$new_hash" ]; then
        echo "RESULT: WASM artifacts are IDENTICAL (no changes detected)."
    else
        echo "RESULT: WASM artifacts DIFFER (bytecode has changed)."
        local size_diff=$((new_size - old_size))
        echo "       Size change: ${size_diff} bytes"
    fi

    echo ""
    echo "═══════════════════════════════════════════════════════════════"
}

case "$MODE" in
    build)
        build_contracts
        ;;
    hash)
        compute_hashes
        ;;
    diff)
        diff_wasm "${2:-}" "${3:-}"
        ;;
    verify)
        verify_wasm "${2:-}"
        ;;
    help|--help|-h)
        print_usage
        ;;
    *)
        echo "Unknown command: ${MODE}"
        print_usage
        exit 1
        ;;
esac
