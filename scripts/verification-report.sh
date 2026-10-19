#!/usr/bin/env bash
# Verification report for Certora specs (Issue #687).
#
# Structure-checks every .spec file referenced by config.json against the
# verification baseline: file exists, has methods{}, has rules, contains
# all expected rule IDs. Emits verification-report.md.
#
# Usage:
#   bash scripts/verification-report.sh
#   CERTORA_DIR=path/to/certora bash scripts/verification-report.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CERTORA_DIR="${CERTORA_DIR:-$REPO_ROOT/stellar-lend/contracts/lending/certora}"
BASELINE="${BASELINE:-$CERTORA_DIR/verification-baseline.json}"
CONFIG="${CONFIG:-$CERTORA_DIR/config.json}"
OUT="${OUT:-$REPO_ROOT/verification-report.md}"

if [[ ! -d "$CERTORA_DIR" ]]; then
  echo "::error::Certora directory not found: $CERTORA_DIR" >&2
  exit 1
fi
if [[ ! -f "$BASELINE" ]]; then
  echo "::error::Baseline not found: $BASELINE" >&2
  exit 1
fi
if [[ ! -f "$CONFIG" ]]; then
  echo "::error::Config not found: $CONFIG" >&2
  exit 1
fi

FAILURES=0
{
  echo "# Formal Verification Structure Report"
  echo
  echo "- Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "- Baseline: \`$(basename "$BASELINE")\`"
  echo "- Config: \`$(basename "$CONFIG")\`"
  echo
  echo "| Spec | Exists | methods{} | Rules | Baseline IDs | Status |"
  echo "|------|--------|-----------|-------|--------------|--------|"
} > "$OUT"

# Extract expected spec filenames from baseline (portable: no jq dependency
# for the common case; fall back to grep). Compatible with bash 3.2 (macOS).
if command -v jq >/dev/null 2>&1; then
  SPECS=$(jq -r '.specs[].file' "$BASELINE")
else
  SPECS=$(grep -oE '"file"[[:space:]]*:[[:space:]]*"[^"]+"' "$BASELINE" | sed -E 's/.*"file"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')
fi

while IFS= read -r spec; do
  [[ -z "$spec" ]] && continue
  path="$CERTORA_DIR/$spec"
  exists="no"
  methods="no"
  rule_count=0
  ids_ok="no"
  status="FAIL"

  if [[ -f "$path" ]]; then
    exists="yes"
    if grep -q 'methods[[:space:]]*{' "$path"; then
      methods="yes"
    fi
    rule_count=$(grep -cE '^[[:space:]]*rule[[:space:]]+' "$path" || true)

    # Check every baseline rule id appears in the file.
    if command -v jq >/dev/null 2>&1; then
      IDS=$(jq -r --arg f "$spec" '.specs[] | select(.file==$f) | .rule_ids[]' "$BASELINE")
    else
      IDS=$(awk -v f="\"$spec\"" '
        $0 ~ f {inblock=1}
        inblock && /rule_ids/ {inids=1}
        inids && /\]/ {inids=0; inblock=0}
        inids {
          while (match($0, /"[A-Z]+-[0-9]+"/)) {
            print substr($0, RSTART+1, RLENGTH-2)
            $0 = substr($0, RSTART+RLENGTH)
          }
        }
      ' "$BASELINE")
    fi

    ids_ok="yes"
    while IFS= read -r id; do
      [[ -z "$id" ]] && continue
      if ! grep -q -- "$id" "$path"; then
        ids_ok="no"
        echo "::error::Missing rule id $id in $spec"
        FAILURES=$((FAILURES + 1))
      fi
    done <<EOF
$IDS
EOF

    if [[ "$methods" != "yes" ]]; then
      echo "::error::$spec missing methods{} block"
      FAILURES=$((FAILURES + 1))
    fi
    if [[ "$rule_count" -eq 0 ]]; then
      echo "::error::$spec has zero rule definitions"
      FAILURES=$((FAILURES + 1))
    fi

    if [[ "$exists" == "yes" && "$methods" == "yes" && "$rule_count" -gt 0 && "$ids_ok" == "yes" ]]; then
      status="PASS"
    fi
  else
    echo "::error::Spec file missing: $spec"
    FAILURES=$((FAILURES + 1))
  fi

  echo "| \`$spec\` | $exists | $methods | $rule_count | $ids_ok | $status |" >> "$OUT"
done <<EOF
$SPECS
EOF

# Verify config lists every baseline spec.
for spec in $SPECS; do
  [[ -z "$spec" ]] && continue
  if ! grep -q -- "$spec" "$CONFIG"; then
    echo "::error::config.json does not reference $spec"
    FAILURES=$((FAILURES + 1))
  fi
done

{
  echo
  if [[ "$FAILURES" -eq 0 ]]; then
    echo "**Result: PASS** — all structure checks succeeded."
  else
    echo "**Result: FAIL** — $FAILURES structure check(s) failed."
  fi
} >> "$OUT"

echo "Report written to $OUT"
if [[ "$FAILURES" -ne 0 ]]; then
  exit 1
fi
echo "All Certora structure checks passed."
