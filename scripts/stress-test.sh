#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────────────────────────────────
# stress-test.sh — Protocol resilience stress testing for stellarlend lending
#
# Runs the lending contract's stress test suite, which validates storage
# layout, indexing, and iteration logic under load (150+ users, multiple
# positions per user, concurrent operation simulation).
#
# Usage:
#   ./scripts/stress-test.sh
#   ./scripts/stress-test.sh --release
# ─────────────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
LENDING_CONTRACT_DIR="${REPO_ROOT}/stellar-lend/contracts/lending"

cd "${REPO_ROOT}"

MODE="debug"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release)
      MODE="release"
      shift
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

echo "═══════════════════════════════════════════════════════════════"
echo "  Stellarlend Protocol Stress Test Suite"
echo "  Mode: ${MODE}"
echo "═══════════════════════════════════════════════════════════════"
echo ""

if ! command -v cargo &> /dev/null; then
  echo "ERROR: cargo not found. Install Rust toolchain first:"
  echo "  https://rustup.rs/"
  exit 1
fi

echo "[1/4] Building lending contract..."
if [[ "${MODE}" == "release" ]]; then
  cargo build -p lending --release --quiet 2>&1
else
  cargo build -p lending --quiet 2>&1
fi
echo "      Build complete."
echo ""

echo "[2/4] Running stress tests (filter: stress)..."
cargo test -p lending stress -- --nocapture 2>&1
echo ""

echo "[3/4] Running invariant tests..."
cargo test -p lending invariant -- --nocapture 2>&1
echo ""

echo "[4/4] Running reentrancy tests..."
cargo test -p lending reentrancy -- --nocapture 2>&1
echo ""

echo "═══════════════════════════════════════════════════════════════"
echo "  All stress tests passed."
echo "═══════════════════════════════════════════════════════════════"
