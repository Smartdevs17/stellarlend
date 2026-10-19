#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# rehearse-upgrade.sh -- rehearse a lending upgrade AND its rollback on the devnet
#
#   npm run devnet                       # once, to create the devnet
#   npm run devnet:rehearse-upgrade      # builds the working tree as the candidate
#   npm run devnet:rehearse-upgrade -- --candidate path/to/candidate.wasm
#
# Executes the procedure in docs/runbooks/upgrade-and-migration.md end to end:
#   snapshot -> upload -> upgrade -> verify hash -> migrate (x2, idempotent)
#   -> compare state -> smoke write -> rollback -> verify hash -> compare state
#
# Every artefact (wasm copies, snapshots, report) lands in
# .devnet/rehearsal/<timestamp>/ so the run can be attached to a release ticket.
# Exits non-zero on the first failed check.
# ---------------------------------------------------------------------------
set -euo pipefail

# shellcheck source=scripts/devnet/lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

CANDIDATE=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [--candidate <wasm>] [--dry-run]

  --candidate <wasm>  Wasm to upgrade to (default: build the working tree)
  --dry-run           Print every command instead of running it
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --candidate) CANDIDATE="${2:?--candidate needs a path}"; shift ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

OUT="${STATE_DIR}/rehearsal/$(date -u +%Y%m%dT%H%M%SZ)"
REPORT="${OUT}/report.md"

admin_invoke() {
  run stellar contract invoke --id "${LENDING_ID}" --source "${ADMIN}" "${NET_ARGS[@]}" -- "$@"
}

view() {
  capture "<${1}>" stellar contract invoke --id "${LENDING_ID}" --source "${ADMIN}" \
    "${NET_ARGS[@]}" --send=no -- "$@"
}

sha() {
  if [ "${DRY_RUN}" -eq 1 ]; then echo "<sha256:$(basename "$1")>"; else sha256sum "$1" | cut -d' ' -f1; fi
}

# Fetch the live contract code and return its hash.
onchain_hash() {
  local file="${OUT}/onchain-$1.wasm"
  run stellar contract fetch --id "${LENDING_ID}" "${NET_ARGS[@]}" --out-file "${file}"
  sha "${file}"
}

expect_eq() {
  local what="$1" got="$2" want="$3"
  [ "${DRY_RUN}" -eq 1 ] && return 0
  [ "${got}" = "${want}" ] || die "${what}: expected ${want}, got ${got}"
  log "  ok: ${what}"
  echo "- [x] ${what}" >> "${REPORT}"
}

# State that must survive a code swap untouched. Only stored values are read
# (no time-dependent views), so two snapshots taken minutes apart compare equal.
snapshot() {
  local file="${OUT}/state-$1.txt" sink
  log "Snapshotting state ($1)..."
  sink="${file}"
  [ "${DRY_RUN}" -eq 1 ] && sink=/dev/null
  {
    echo "## admin";   view get_admin
    echo "## pool";    view get_pool_state
    echo "## breaker"; view get_utilization_breaker
    for name in alice bob carol; do
      local addr
      case "${name}" in
        alice) addr="${ALICE_ADDR}" ;; bob) addr="${BOB_ADDR}" ;; carol) addr="${CAROL_ADDR}" ;;
      esac
      echo "## ${name} debt";       view get_user_debt --user "${addr}"
      echo "## ${name} collateral"; view get_user_collateral --user "${addr}"
      echo "## ${name} deposit";    view get_user_collateral_deposit --user "${addr}" --asset "${XLM_ID}"
    done
  } > "${sink}"
  echo "${file}"
}

expect_same_state() {
  local what="$1" a="$2" b="$3"
  [ "${DRY_RUN}" -eq 1 ] && return 0
  if ! diff -u "${a}" "${b}" > "${OUT}/diff-$(basename "${b}" .txt).patch"; then
    die "${what}: state changed (see ${OUT}/diff-$(basename "${b}" .txt).patch)"
  fi
  log "  ok: ${what}"
  echo "- [x] ${what}" >> "${REPORT}"
}

main() {
  if [ "${DRY_RUN}" -eq 0 ]; then
    command -v stellar >/dev/null || die "stellar CLI not found"
    [ -f "${STATE_FILE}" ] || die "no devnet state at ${STATE_FILE}; run: npm run devnet"
    rpc_healthy || die "devnet RPC ${RPC_URL} is not healthy; run: npm run devnet"
    LENDING_ID="$(state_field lending)"
    XLM_ID="$(state_field xlm)"
    USDC_ID="$(state_field usdc)"
    PREV_HASH="$(state_field lendingWasmHash)"
  else
    LENDING_ID="<lending-contract-id>"; XLM_ID="<xlm-contract-id>"; USDC_ID="<usdc-contract-id>"
    PREV_HASH="<sha256:previous.wasm>"
  fi
  ADMIN_ADDR="$(capture "<${ADMIN}-address>" stellar keys address "${ADMIN}")"
  ALICE_ADDR="$(capture "<${ALICE}-address>" stellar keys address "${ALICE}")"
  BOB_ADDR="$(capture "<${BOB}-address>" stellar keys address "${BOB}")"
  CAROL_ADDR="$(capture "<${CAROL}-address>" stellar keys address "${CAROL}")"

  run mkdir -p "${OUT}"
  if [ "${DRY_RUN}" -eq 0 ]; then
    printf '# Upgrade rehearsal %s\n\n- contract: `%s`\n- rpc: %s\n\n## Checks\n\n' \
      "$(basename "${OUT}")" "${LENDING_ID}" "${RPC_URL}" > "${REPORT}"
  fi

  # 1. Record and keep the currently deployed code: it is the rollback target.
  log "Step 1: record current code"
  local live
  live="$(onchain_hash before)"
  expect_eq "live code matches recorded hash" "${live}" "${PREV_HASH}"
  run cp "${OUT}/onchain-before.wasm" "${OUT}/previous.wasm"

  # 2. Candidate wasm.
  log "Step 2: prepare candidate"
  if [ -z "${CANDIDATE}" ]; then
    run stellar contract build --manifest-path "${LENDING_MANIFEST}"
    CANDIDATE="${WASM}"
  fi
  run cp "${CANDIDATE}" "${OUT}/candidate.wasm"
  local cand_hash
  cand_hash="$(sha "${OUT}/candidate.wasm")"

  # 3. Pre-upgrade snapshot.
  local pre
  pre="$(snapshot 1-pre-upgrade)"

  # 4. Upload + upgrade.
  log "Step 4: upload candidate and upgrade"
  local uploaded
  uploaded="$(capture "${cand_hash}" stellar contract upload --wasm "${OUT}/candidate.wasm" \
    --source "${ADMIN}" "${NET_ARGS[@]}")"
  expect_eq "uploaded hash matches candidate sha256" "${uploaded}" "${cand_hash}"
  admin_invoke upgrade --admin "${ADMIN_ADDR}" --new_wasm_hash "${cand_hash}"
  expect_eq "live code is candidate after upgrade" "$(onchain_hash after-upgrade)" "${cand_hash}"

  # 5. Storage migration. Must be idempotent: the second run writes nothing.
  log "Step 5: migrate storage"
  admin_invoke migrate_pool_state --admin "${ADMIN_ADDR}"
  local second
  second="$(capture 0 stellar contract invoke --id "${LENDING_ID}" --source "${ADMIN}" \
    "${NET_ARGS[@]}" -- migrate_pool_state --admin "${ADMIN_ADDR}")"
  expect_eq "second migrate_pool_state is a no-op" "${second}" "0"

  # 6. State preserved across the upgrade.
  local post
  post="$(snapshot 2-post-upgrade)"
  expect_same_state "state preserved across upgrade" "${pre}" "${post}"

  # 7. Smoke-test the write path on the new code (1 USDC borrow + repay).
  log "Step 7: smoke-test write path"
  run stellar contract invoke --id "${LENDING_ID}" --source "${ALICE}" "${NET_ARGS[@]}" -- \
    borrow --user "${ALICE_ADDR}" --asset "${USDC_ID}" --amount "${UNIT}" \
    --collateral_asset "${XLM_ID}" --collateral_amount $((2 * UNIT))
  run stellar contract invoke --id "${LENDING_ID}" --source "${ALICE}" "${NET_ARGS[@]}" -- \
    repay --user "${ALICE_ADDR}" --asset "${USDC_ID}" --amount "${UNIT}"
  [ "${DRY_RUN}" -eq 0 ] && echo "- [x] borrow + repay succeed on upgraded code" >> "${REPORT}"

  # 8. Rollback to the recorded previous code.
  local before_rb
  before_rb="$(snapshot 3-pre-rollback)"
  log "Step 8: roll back"
  local reup
  reup="$(capture "${PREV_HASH}" stellar contract upload --wasm "${OUT}/previous.wasm" \
    --source "${ADMIN}" "${NET_ARGS[@]}")"
  expect_eq "previous wasm still installable" "${reup}" "${PREV_HASH}"
  admin_invoke upgrade --admin "${ADMIN_ADDR}" --new_wasm_hash "${PREV_HASH}"
  expect_eq "live code is previous after rollback" "$(onchain_hash after-rollback)" "${PREV_HASH}"

  local after_rb
  after_rb="$(snapshot 4-post-rollback)"
  expect_same_state "state preserved across rollback" "${before_rb}" "${after_rb}"

  if [ "${DRY_RUN}" -eq 0 ]; then
    printf '\n## Hashes\n\n- previous: `%s`\n- candidate: `%s`\n\nResult: PASS\n' \
      "${PREV_HASH}" "${cand_hash}" >> "${REPORT}"
  fi
  if [ "${DRY_RUN}" -eq 1 ]; then
    log "(dry-run) rehearsal plan printed; nothing was executed"
  else
    log "Rehearsal PASSED. Report: ${REPORT}"
  fi
}

main
