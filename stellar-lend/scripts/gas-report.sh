#!/usr/bin/env bash
#
# gas-report.sh — Generate a pull-request gas optimization report.
#
# Wires together the StellarLend benchmark suite (run_benchmarks) and the
# regression gate (check_gas_benchmarks.py) into a single, CI-friendly
# report that covers issue #718:
#   - Gas report generation for PRs
#   - Gas regression detection
#   - Gas optimization suggestions
#   - Gas report CI integration + formatting + comparison + documentation
#
# Outputs:
#   benchmarks/latest.json        — raw JSON from the benchmark suite (PR state)
#   benchmarks/report.md          — human-readable PR gas report (posted/marked up)
#   benchmarks/history.json       — appended trend history (append-only)
#
# Exit codes:
#   0  all benchmarks within regression budget
#   1  gas regressions detected (fails CI / PR check)
#   2  benchmark suite failed to run or produce output
#
set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration (override via environment variables)
# ---------------------------------------------------------------------------
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BENCH_DIR="${BENCH_DIR:-${ROOT_DIR}/benchmarks}"
BASELINE="${BASELINE:-${ROOT_DIR}/benchmarks/baseline.json}"
LATEST="${LATEST:-${BENCH_DIR}/latest.json}"
REPORT_MD="${REPORT_MD:-${BENCH_DIR}/report.md}"
HISTORY="${HISTORY:-${BENCH_DIR}/history.json}"
MAX_REGRESSION_PCT="${MAX_REGRESSION_PCT:-$(grep -E '^\s*threshold\s*=' "${BENCH_DIR}/config.toml" 2>/dev/null | tail -1 | grep -oE '[0-9]+(\.[0-9]+)?' || echo 5.0)}"

log()  { printf '[gas-report] %s\n' "$*" >&2; }

# ---------------------------------------------------------------------------
# 1. Run the benchmark suite on the current (PR) code
# ---------------------------------------------------------------------------
log "running benchmark suite (baseline=${BASELINE})"
(
    cd "${ROOT_DIR}"
    cargo run --quiet --bin run_benchmarks -- --output "${LATEST}"
) || {
    log "ERROR: benchmark suite failed to run"
    exit 2
}

if [[ ! -f "${LATEST}" ]] || ! python3 -c "import json,sys; json.load(open(sys.argv[1]))" "${LATEST}"; then
    log "ERROR: benchmark suite produced no valid JSON at ${LATEST}"
    exit 2
fi

# ---------------------------------------------------------------------------
# 2. Detect gas regressions against the baseline
# ---------------------------------------------------------------------------
log "comparing ${LATEST} against ${BASELINE} (threshold=${MAX_REGRESSION_PCT}%)"
if python3 "${ROOT_DIR}/scripts/check_gas_benchmarks.py" \
    --baseline "${BASELINE}" \
    --current  "${LATEST}" \
    --max-regression-pct "${MAX_REGRESSION_PCT}" > /tmp/gas-regression.out 2>&1; then
    REGRESSION=0
else
    REGRESSION=$?
fi
cat /tmp/gas-regression.out >&2

# ---------------------------------------------------------------------------
# 3. Append the run to the trend history (append-only)
# ---------------------------------------------------------------------------
python3 - "${HISTORY}" "${LATEST}" <<'PY'
import json, sys
history_path, latest_path = sys.argv[1], sys.argv[2]
with open(latest_path) as f:
    latest = json.load(f)
try:
    with open(history_path) as f:
        history = json.load(f)
except (FileNotFoundError, json.JSONDecodeError):
    history = []
history.append({
    "timestamp": latest.get("timestamp"),
    "total_benchmarks": latest.get("total_benchmarks", 0),
    "passed": latest.get("passed", 0),
    "failed": latest.get("failed", 0),
})
# Cap history at the most recent 250 runs to keep the file lean.
history = history[-250:]
with open(history_path, "w") as f:
    json.dump(history, f, indent=2)
    f.write("\n")
PY

# ---------------------------------------------------------------------------
# 4. Format a human-readable Markdown report for the PR
# ---------------------------------------------------------------------------
python3 - "${LATEST}" "${REPORT_MD}" <<'PY'
import json, sys
latest_path, out_path = sys.argv[1], sys.argv[2]
with open(latest_path) as f:
    r = json.load(f)

lines = []
lines.append("## Gas Report")
lines.append("")
lines.append(f"- **Benchmarks run:** `{r.get('total_benchmarks', 0)}`")
lines.append(f"- **Passed:** `{r.get('passed', 0)}` | **Over budget:** `{r.get('failed', 0)}`")
lines.append("")
lines.append("| Contract | Operations | Max insns | Min insns | Avg insns | Over budget |")
lines.append("| --- | ---: | ---: | ---: | ---: | ---: |")
for name in sorted(r.get("summary_by_contract", {}).keys()):
    c = r["summary_by_contract"][name]
    lines.append(
        f"| {c['contract']} | {c['total_operations']} | {c['max_instructions']} "
        f"| {c['min_instructions']} | {c['avg_instructions']} | {c['over_budget_count']} |"
    )
lines.append("")
lines.append("### Per-operation detail")
lines.append("")
lines.append("| Operation | Instructions | Memory (B) | Storage R/W | In budget |")
lines.append("| --- | ---: | ---: | ---: | --- |")
for item in r.get("results", []):
    lines.append(
        f"| {item.get('operation', '')} | {item.get('instructions', 0)} "
        f"| {item.get('memory_bytes', 0)} | {item.get('storage_reads', 0)}/{item.get('storage_writes', 0)} "
        f"| {'yes' if item.get('within_budget') else '**no**'} |"
    )
lines.append("")

# Optimization suggestions for operations that went over their budget.
over = [i for i in r.get("results", []) if not i.get("within_budget")]
if over:
    lines.append("### Optimization suggestions")
    lines.append("")
    for item in over:
        lines.append(f"- `{item.get('operation', '')}` is over budget — review its storage access pattern "
                     "(e.g. batch reads, pack fields into fewer storage slots, early-exit on unprofitable paths).")
    lines.append("")

with open(out_path, "w") as f:
    f.write("\n".join(lines) + "\n")
print(f"report written to {out_path}")
PY

log "gas report generated at ${REPORT_MD}"

# ---------------------------------------------------------------------------
# 5. Exit signal for CI (0 = clean, 1 = regressions)
# ---------------------------------------------------------------------------
if [[ "${REGRESSION}" -ne 0 ]]; then
    log "gas regressions detected (exit ${REGRESSION})"
    exit 1
fi

log "no gas regressions detected"
exit 0
