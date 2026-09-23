#!/usr/bin/env bash
#
# chaos_report.sh — Chaos test report generation (Issue #689).
#
# Converts Jest JSON results into a markdown chaos experiment report with
# suite totals, pass/fail tables, and an experiment log suitable for CI
# artifact upload.
#
# Usage:
#   chaos_report.sh [input.json] [output.md]
#
# Defaults:
#   input   tests/chaos/chaos-report.json
#   output  tests/chaos/chaos-report.md
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INPUT="${1:-${ROOT_DIR}/tests/chaos/chaos-report.json}"
OUTPUT="${2:-${ROOT_DIR}/tests/chaos/chaos-report.md}"

if [[ ! -f "$INPUT" ]]; then
  echo "chaos_report: input not found: $INPUT" >&2
  exit 2
fi

python3 - "$INPUT" "$OUTPUT" <<'PY'
import json, os, sys, datetime

input_path, output_path = sys.argv[1], sys.argv[2]

with open(input_path) as f:
    data = json.load(f)

# Jest JSON may be a single object or (with multiple projects) a list.
suites = data if isinstance(data, list) else [data]

rows = []
failures = []

for suite in suites:
    for tr in suite.get("testResults", []):
        name = os.path.basename(tr.get("name", ""))
        for assertion in tr.get("assertionResults", []):
            status = assertion.get("status", "unknown")
            title = assertion.get("fullName") or assertion.get("title", "")
            rows.append((name, title, status))
            if status == "failed":
                failures.append({
                    "suite": name,
                    "test": title,
                    "message": (assertion.get("failureMessages") or [""])[0][:500],
                })

passed = sum(1 for _, _, s in rows if s == "passed")
failed = sum(1 for _, _, s in rows if s == "failed")
skipped = sum(1 for _, _, s in rows if s in ("pending", "skipped", "todo"))
total = len(rows)
status = "PASS" if failed == 0 else "FAIL"
if total == 0:
    status = "EMPTY"

now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")

lines = [
    "# Chaos Engineering Report",
    "",
    f"- **Generated:** {now}",
    f"- **Overall status:** `{status}`",
    f"- **Total:** {total} · **Passed:** {passed} · **Failed:** {failed} · **Skipped:** {skipped}",
    f"- **Source:** `{os.path.relpath(input_path)}`",
    "",
    "## Experiment Results",
    "",
    "| Suite | Test | Status |",
    "| --- | --- | --- |",
]
for suite, title, st in rows:
    lines.append(f"| {suite} | {title} | {st} |")

if failures:
    lines += ["", "## Failures", ""]
    for f in failures:
        lines.append(f"### {f['suite']} — {f['test']}")
        lines.append("")
        lines.append("```")
        lines.append(f["message"])
        lines.append("```")
        lines.append("")

lines += [
    "## Experiments Covered",
    "",
    "| Experiment | Steady-state hypothesis | Halt criteria |",
    "| --- | --- | --- |",
    "| Network partition | Reads keep serving from cache/fallback while a partition is active | Data inconsistency after heal |",
    "| RPC outage | API returns degraded status, writes rejected with 503, reads still served | Unhandled 5xx on reads |",
    "| Slow RPC / latency | Retries with backoff eventually submit transactions | Retry exhaustion without recovery |",
    "| Oracle feed disruption | Stale prices rejected; fallback/circuit breaker engages | Stale price accepted by contract |",
    "| Recovery | All failure modes clear; metrics show successful recoveries | dataConsistency flag false |",
    "",
    "See `docs/chaos-engineering.md` for the full experiment registry and runbook.",
    "",
]

os.makedirs(os.path.dirname(output_path) or ".", exist_ok=True)
with open(output_path, "w") as f:
    f.write("\n".join(lines))

print(f"chaos_report: wrote {output_path} ({status}, {passed}/{total} passed)")
sys.exit(0 if status != "FAIL" else 1)
PY
