#!/usr/bin/env bash
#
# gas-report.sh — StellarLend gas reporting & regression gate (issue #718).
#
# Single entrypoint used by both CI (.github/workflows/gas-report.yml) and
# local developers. Subcommands:
#
#   baseline                   Print the committed baseline report as JSON.
#                              Falls back to the measured `gas-baseline.json`
#                              data when baseline.json has no results yet.
#   baseline --update          Re-run the suite and adopt the result as the
#                              committed baseline.
#   report                     Run the suite on the current tree and print the
#                              raw BenchmarkReport JSON to stdout.
#   compare <base> <current>   Emit a Markdown diff (stdout) and write
#                              `comparison.json` beside the current report.
#   check <comparison.json>    Emit {total, improvements, regressions,
#                              average_change} as JSON (stdout).
#   (no args)                  Full local flow: report + compare vs baseline +
#                              regression exit code.
#
# Exit codes: 0 clean · 1 gas regressions · 2 suite/kite failure.
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SL_DIR="${ROOT_DIR}/stellar-lend"
BASELINE="${BASELINE:-${SL_DIR}/benchmarks/baseline.json}"
GAS_BASELINE_RAW="${SL_DIR}/benchmarks/gas-baseline.json"
PUBLIC_FUNCTIONS="${SL_DIR}/benchmarks/public-functions.json"
RESULTS="${RESULTS:-${SL_DIR}/benchmark-results.json}"
MAX_REGRESSION_PCT="${MAX_REGRESSION_PCT:-10}"

log() { printf '[gas-report] %s\n' "$*" >&2; }

# ---------------------------------------------------------------------------
# baseline: normalize the committed reference into a BenchmarkReport JSON.
# ---------------------------------------------------------------------------
cmd_baseline() {
    if [[ "${1:-}" == "--update" ]]; then
        "${ROOT_DIR}/run-benchmarks.sh"
        cp "${RESULTS}" "${BASELINE}"
        log "baseline updated at ${BASELINE}"
    fi

    if [[ ! -f "${BASELINE}" ]]; then
        log "ERROR: baseline missing at ${BASELINE}"
        exit 2
    fi

    # report.py-style baseline + raw JSON handling
    python3 - "${BASELINE}" "${GAS_BASELINE_RAW}" "${PUBLIC_FUNCTIONS}" <<'PY'
import json, sys
baseline_path, raw_path, coverage_path = sys.argv[1], sys.argv[2], sys.argv[3]

with open(baseline_path) as f:
    base = json.load(f)

results = base.get("results", [])
if results:
    print(json.dumps(base)); raise SystemExit(0)

# Empty stub baseline: build a meaningful reference from the measured raw data.
try:
    with open(raw_path) as f:
        raw = json.load(f)
except FileNotFoundError:
    print(json.dumps(base)); raise SystemExit(0)

contract = raw.get("contract", "hello_world")
out = []
for item in raw.get("benchmarks", []):
    op = f"{contract}::{item.get('operation', '')}"
    out.append({
        "operation": op,
        "contract": contract,
        "description": item.get("scenario", ""),
        "instructions": int(item.get("cpu_insns", 0)),
        "memory_bytes": int(item.get("mem_bytes", 0)),
        "storage_reads": 0,
        "storage_writes": 0,
        "within_budget": True,
    })
print(json.dumps({
    "version": "0.1.0",
    "timestamp": base.get("timestamp", ""),
    "total_benchmarks": len(out),
    "passed": len(out),
    "failed": 0,
    "results": out,
    "summary_by_contract": {
        contract: {
            "contract": contract,
            "total_operations": len(out),
            "max_instructions": max((r["instructions"] for r in out), default=0),
            "min_instructions": min((r["instructions"] for r in out), default=0),
            "avg_instructions": sum(r["instructions"] for r in out) // len(out) if out else 0,
            "over_budget_count": 0,
        }
    },
}))
PY
}

# ---------------------------------------------------------------------------
# report: run the suite on the current tree → raw BenchmarkReport JSON.
# ---------------------------------------------------------------------------
cmd_report() {
    log "running benchmark suite (release)"
    (
        cd "${SL_DIR}"
        cargo run -p stellarlend-benchmarks --bin run_benchmarks --release -- \
            --output "../${RESULTS}" 2>&1 | sed 's/^/[bench] /' >&2
    )
    if [[ ! -f "${RESULTS}" ]]; then
        log "ERROR: benchmark suite produced no report at ${RESULTS}"
        exit 2
    fi
    cat "${RESULTS}"
}

# ---------------------------------------------------------------------------
# compare: markdown diff (stdout) + comparison.json sidecar.
# ---------------------------------------------------------------------------
cmd_compare() {
    [[ $# -ge 2 ]] || { echo "usage: gas-report compare <base> <current>" >&2; exit 2; }
    python3 - "${1}" "${2}" <<'PY'
import json, os, sys
base_path, cur_path = sys.argv[1], sys.argv[2]
with open(base_path) as f: base = json.load(f)
with open(cur_path) as f: cur = json.load(f)

def ix(r):
    return {i.get("operation", ""): i for i in r.get("results", [])}

base_ix, cur_ix = ix(base), ix(cur)
ops = sorted(set(base_ix) | set(cur_ix))
rows = []
for op in ops:
    b, c = base_ix.get(op), cur_ix.get(op)
    if b is None or c is None:
        rows.append({"operation": op, "baseline": None, "current": None,
                     "change_pct": None, "status": "missing"})
        continue
    bv, cv = int(b.get("instructions", 0)), int(c.get("instructions", 0))
    change = ((cv - bv) / bv * 100.0) if bv else None
    status = "improved" if (change is not None and change < -1) else (
        "regression" if (change is not None and change > 0) else "flat")
    rows.append({"operation": op, "baseline": bv, "current": cv,
                 "change_pct": change, "status": status})

improvements = sum(1 for r in rows if r["status"] == "improved")
regressions = sum(1 for r in rows if r["status"] == "regression")
changes = [abs(r["change_pct"]) for r in rows if r["change_pct"] is not None]
average = sum(changes) / len(changes) if changes else 0.0

# Markdown to stdout
lines = ["| Operation | Baseline | Current | Change | Status |", "| --- | ---: | ---: | ---: | --- |"]
for r in rows:
    if r["status"] == "missing":
        lines.append(f"| {r['operation']} | — | — | — | missing |"); continue
    sign = "+" if (r["change_pct"] or 0) > 0 else ""
    lines.append(f"| {r['operation']} | {r['baseline']:,} | {r['current']:,} | "
                 f"{sign}{r['change_pct']:.2f}% | {r['status']} |")
print("\n".join(lines))

sidecar = os.path.join(os.path.dirname(cur_path), "comparison.json")
with open(sidecar, "w") as f:
    json.dump({"total": len(rows), "improvements": improvements,
               "regressions": regressions, "average_change": average,
               "rows": rows}, f, indent=2)
log(f"wrote {sidecar}")
PY
}

# ---------------------------------------------------------------------------
# check: regression gate JSON for CI.
# ---------------------------------------------------------------------------
cmd_check() {
    [[ $# -ge 1 ]] || { echo "usage: gas-report check <comparison.json>" >&2; exit 2; }
    python3 - "${1}" "${MAX_REGRESSION_PCT}" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    cmp_ = json.load(f)
threshold = float(sys.argv[2])
total, improvements, regressions = cmp_["total"], cmp_["improvements"], cmp_["regressions"]
regressed = [r["operation"] for r in cmp_.get("rows", [])
             if r.get("change_pct") is not None and r["change_pct"] > threshold]
out = {
    "total": total,
    "improvements": improvements,
    "regressions": regressions,
    "average_change": round(cmp_["average_change"], 2),
    "threshold_pct": threshold,
    "failed": len(regressed),
    "regressed_operations": regressed,
    "ok": len(regressed) == 0,
}
print(json.dumps(out))
raise SystemExit(0 if out["ok"] else 1)
PY
}

# ---------------------------------------------------------------------------
case "${1:-}" in
    baseline) shift; cmd_baseline "$@" ;;
    report)   shift; cmd_report "$@" ;;
    compare)  shift; cmd_compare "$@" ;;
    check)    shift; cmd_check "$@" ;;
    "")
        cmd_report > /tmp/gas-current.json
        rm -f /tmp/comparison.json
        if cmd_compare "${BASELINE}" /tmp/gas-current.json > /tmp/gas-comparison.md; then :; fi
        cmd_check /tmp/comparison.json
        ;;
    *) echo "unknown subcommand: $1" >&2; exit 2 ;;
esac