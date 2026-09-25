#!/usr/bin/env bash
# Gas regression testing wrapper (#1065).
#
# Builds + runs the gas benchmark suite and compares the results against the
# committed baseline, failing when any benchmark regresses beyond a threshold.
# Reuses the existing benchmark tooling (run-benchmarks.sh --compare +
# scripts/gas_benchmark_report.py) so the exact same gate runs locally and in CI.
#
# Usage:
#   ./scripts/gas-regression.sh                # default threshold: 10% (10.0)
#   ./scripts/gas-regression.sh --threshold 5  # stricter gate
#   GAS_REGRESSION_THRESHOLD=5 ./scripts/gas-regression.sh
#
# Exit codes:
#   0  no regressions over threshold
#   1  one or more benchmarks regressed beyond the threshold
#   2  usage / environment error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

THRESHOLD="${GAS_REGRESSION_THRESHOLD:-${1:-10.0}}"
if [[ "$1" == "--threshold" ]]; then
    THRESHOLD="${2:-10.0}"
fi

BASELINE="${REPO_ROOT}/stellar-lend/benchmarks/baseline.json"

if ! command -v cargo >/dev/null 2>&1; then
    echo "ERROR: cargo not found." >&2
    exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
    echo "ERROR: python3 not found (required for the regression report)." >&2
    exit 2
fi
if [ ! -f "$BASELINE" ]; then
    echo "ERROR: no committed baseline found at stellar-lend/benchmarks/baseline.json." >&2
    echo "       Run ./run-benchmarks.sh --update-baseline and commit the result first." >&2
    exit 2
fi

echo "═══════════════════════════════════════════════════════════════"
echo "  Gas Regression Check  (threshold: ${THRESHOLD}%)"
echo "  Baseline: stellar-lend/benchmarks/baseline.json"
echo "═══════════════════════════════════════════════════════════════"

# Run the shareable pipeline: build + bench + report, with the baseline
# comparison and regression gate enabled. GAS_REG_MAX_PCT flows through to
# run-benchmarks.sh and then to gas_benchmark_report.py's --max-regression-pct.
if ! (cd "$REPO_ROOT" && GAS_REG_MAX_PCT="$THRESHOLD" bash run-benchmarks.sh --compare); then
    echo "::error::Gas regression detected (threshold ${THRESHOLD}%)."
    echo "            Inspect stellar-lend/benchmark-results.json for details."
    exit 1
fi

echo ""
echo "No gas regressions beyond ${THRESHOLD}% against the committed baseline."
exit 0