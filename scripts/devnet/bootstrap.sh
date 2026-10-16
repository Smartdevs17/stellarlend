#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# bootstrap.sh -- one-command StellarLend local devnet
#
#   npm run devnet            (or: bash scripts/devnet/bootstrap.sh)
#
# 1. Starts a local Stellar quickstart node (via `stellar container start`)
# 2. Creates and funds devnet identities (admin + three users)
# 3. Builds the lending contract and deploys it
# 4. Deploys a USDC asset contract and resolves the native XLM asset contract
# 5. Initializes the protocol and seeds deposits, borrows and a repayment
# 6. Enables the utilization circuit breaker
# 7. Writes everything to .devnet/state.json and prints a summary
#
# Re-running against a live devnet is a no-op that prints the summary; pass
# --reset to stop the node and start from an empty ledger.
#
# Docs: docs/devnet.md
# ---------------------------------------------------------------------------
set -euo pipefail

# shellcheck source=scripts/devnet/lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

RESET=0
SKIP_BUILD=0
START_NODE=1

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

  --reset        Stop the local node and wipe .devnet/ before bootstrapping
  --skip-build   Deploy the existing wasm instead of rebuilding
  --no-node      Use an already-running RPC (e.g. docker/devnet compose) at \$DEVNET_RPC_URL
  --dry-run      Print every command instead of running it (no network needed)
  -h, --help     Show this help

Environment overrides: DEVNET_RPC_URL, DEVNET_FRIENDBOT_URL, DEVNET_PASSPHRASE,
DEVNET_CONTAINER, DEVNET_KEY_PREFIX, DEVNET_STATE_DIR, DEVNET_WASM, DEVNET_HEALTH_TIMEOUT
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --reset) RESET=1 ;;
    --skip-build) SKIP_BUILD=1 ;;
    --no-node) START_NODE=0 ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------
preflight() {
  [ "${DRY_RUN}" -eq 1 ] && return 0
  command -v stellar >/dev/null || die "stellar CLI not found. Install: cargo install --locked stellar-cli"
  command -v curl >/dev/null || die "curl not found"
  local v
  v="$(stellar --version | head -n1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -n1)"
  version_ge "${v}" "${MIN_STELLAR_CLI}" \
    || die "stellar CLI ${v} is too old; soroban-sdk 27 needs >= ${MIN_STELLAR_CLI}"
  if [ "${START_NODE}" -eq 1 ]; then
    command -v docker >/dev/null || die "docker not found (or pass --no-node with a running RPC)"
  fi
}

wait_for_rpc() {
  if [ "${DRY_RUN}" -eq 1 ]; then
    log "(dry-run) would wait for ${RPC_URL} to report healthy"
    return 0
  fi
  log "Waiting for RPC at ${RPC_URL} (timeout ${HEALTH_TIMEOUT}s)..."
  local waited=0
  until rpc_healthy; do
    [ "${waited}" -ge "${HEALTH_TIMEOUT}" ] && die "RPC not healthy after ${HEALTH_TIMEOUT}s"
    sleep 3
    waited=$((waited + 3))
  done
  log "RPC is healthy."
}

# ---------------------------------------------------------------------------
# Node
# ---------------------------------------------------------------------------
reset_devnet() {
  log "Resetting devnet..."
  if [ "${START_NODE}" -eq 1 ]; then
    run stellar container stop "${CONTAINER}" || true
  fi
  run rm -rf "${STATE_DIR}"
}

start_node() {
  if [ "${START_NODE}" -eq 0 ]; then
    log "--no-node: using existing RPC at ${RPC_URL}"
  elif [ "${DRY_RUN}" -eq 0 ] && rpc_healthy; then
    log "Local node already running."
  else
    log "Starting local quickstart node '${CONTAINER}'..."
    # `unlimited` lifts per-tx and wasm-size limits so the full lending contract deploys.
    run stellar container start "${CONTAINER}" --limits unlimited
  fi
  wait_for_rpc
}

# ---------------------------------------------------------------------------
# Identities
# ---------------------------------------------------------------------------
ensure_identity() {
  local name="$1"
  if [ "${DRY_RUN}" -eq 0 ] && stellar keys address "${name}" >/dev/null 2>&1; then
    :
  else
    run stellar keys generate "${name}" >/dev/null
  fi
  local addr
  addr="$(capture "<${name}-address>" stellar keys address "${name}")"
  log "Funding ${name} (${addr}) via friendbot..."
  run curl -sf -o /dev/null "${FRIENDBOT_URL}?addr=${addr}" \
    || log "  friendbot refused ${name} (already funded?)"
  echo "${addr}"
}

# ---------------------------------------------------------------------------
# Contracts
# ---------------------------------------------------------------------------
build_contract() {
  if [ "${SKIP_BUILD}" -eq 1 ]; then
    log "--skip-build: using ${WASM}"
  else
    log "Building lending contract..."
    run stellar contract build --manifest-path "${LENDING_MANIFEST}"
  fi
  if [ "${DRY_RUN}" -eq 0 ] && [ ! -f "${WASM}" ]; then
    die "wasm not found at ${WASM}"
  fi
}

wasm_hash() {
  if [ "${DRY_RUN}" -eq 1 ]; then
    echo "<wasm-hash>"
  else
    sha256sum "${WASM}" | cut -d' ' -f1
  fi
}

deploy_lending() {
  log "Deploying lending contract..."
  capture "<lending-contract-id>" stellar contract deploy \
    --wasm "${WASM}" --source "${ADMIN}" "${NET_ARGS[@]}"
}

asset_contract_id() {
  local asset="$1" placeholder="$2"
  capture "${placeholder}" stellar contract id asset --asset "${asset}" "${NET_ARGS[@]}"
}

deploy_usdc() {
  local asset="USDC:${ADMIN_ADDR}"
  log "Deploying USDC asset contract (${asset})..."
  if ! capture "<usdc-contract-id>" \
    stellar contract asset deploy --asset "${asset}" --source "${ADMIN}" "${NET_ARGS[@]}"; then
    log "  USDC asset contract already deployed; resolving its id"
    asset_contract_id "${asset}" "<usdc-contract-id>"
  fi
}

# invoke <source-identity> <fn> [--arg value ...]
invoke() {
  local source="$1"; shift
  run stellar contract invoke --id "${LENDING_ID}" --source "${source}" "${NET_ARGS[@]}" -- "$@"
}

# Read-only call: simulated, never submitted.
view() {
  run stellar contract invoke --id "${LENDING_ID}" --source "${ADMIN}" "${NET_ARGS[@]}" \
    --send=no -- "$@"
}

# ---------------------------------------------------------------------------
# Seed
# ---------------------------------------------------------------------------
seed() {
  log "Initializing protocol..."
  # Debt ceiling 1,000,000 USDC; minimum borrow 1 USDC.
  invoke "${ADMIN}" initialize \
    --admin "${ADMIN_ADDR}" \
    --debt_ceiling $((1000000 * UNIT)) \
    --min_borrow_amount $((1 * UNIT))
  invoke "${ADMIN}" initialize_deposit_settings \
    --deposit_cap $((10000000 * UNIT)) \
    --min_deposit_amount $((1 * UNIT))

  log "Seeding positions..."
  # alice: vault deposit + healthy 200% borrow
  invoke "${ALICE}" deposit --user "${ALICE_ADDR}" --asset "${XLM_ID}" --amount $((50000 * UNIT))
  invoke "${ALICE}" borrow --user "${ALICE_ADDR}" --asset "${USDC_ID}" --amount $((25000 * UNIT)) \
    --collateral_asset "${XLM_ID}" --collateral_amount $((50000 * UNIT))
  # bob: borrow at exactly the 150% minimum collateral ratio
  invoke "${BOB}" borrow --user "${BOB_ADDR}" --asset "${USDC_ID}" --amount $((40000 * UNIT)) \
    --collateral_asset "${XLM_ID}" --collateral_amount $((60000 * UNIT))
  # carol: deposit, borrow, then partially repay
  invoke "${CAROL}" deposit --user "${CAROL_ADDR}" --asset "${XLM_ID}" --amount $((20000 * UNIT))
  invoke "${CAROL}" borrow --user "${CAROL_ADDR}" --asset "${USDC_ID}" --amount $((5000 * UNIT)) \
    --collateral_asset "${XLM_ID}" --collateral_amount $((10000 * UNIT))
  invoke "${CAROL}" repay --user "${CAROL_ADDR}" --asset "${USDC_ID}" --amount $((2000 * UNIT))

  log "Enabling utilization circuit breaker (trip 90%, reset 80%, 10k USDC/borrow while tripped)..."
  invoke "${ADMIN}" set_utilization_breaker --admin "${ADMIN_ADDR}" --config \
    "{\"enabled\":true,\"trip_utilization_bps\":9000,\"reset_utilization_bps\":8000,\"max_borrow_while_tripped\":\"$((10000 * UNIT))\"}"
}

verify() {
  log "Verifying seeded state..."
  for pair in "alice:${ALICE_ADDR}" "bob:${BOB_ADDR}" "carol:${CAROL_ADDR}"; do
    log "  ${pair%%:*} position:"
    view get_user_position --user "${pair#*:}" >&2
  done
  log "  utilization breaker:"
  view get_utilization_breaker >&2
}

write_state() {
  local hash="$1"
  if [ "${DRY_RUN}" -eq 1 ]; then
    log "(dry-run) would write ${STATE_FILE}"
    return 0
  fi
  mkdir -p "${STATE_DIR}"
  cat > "${STATE_FILE}" <<EOF
{
  "network": {
    "rpcUrl": "${RPC_URL}",
    "networkPassphrase": "${PASSPHRASE}",
    "friendbotUrl": "${FRIENDBOT_URL}"
  },
  "contracts": {
    "lending": "${LENDING_ID}",
    "lendingWasmHash": "${hash}",
    "xlm": "${XLM_ID}",
    "usdc": "${USDC_ID}"
  },
  "accounts": {
    "admin": { "identity": "${ADMIN}", "address": "${ADMIN_ADDR}" },
    "alice": { "identity": "${ALICE}", "address": "${ALICE_ADDR}" },
    "bob": { "identity": "${BOB}", "address": "${BOB_ADDR}" },
    "carol": { "identity": "${CAROL}", "address": "${CAROL_ADDR}" }
  },
  "createdAt": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF
}

summary() {
  cat >&2 <<EOF

============================================================
 StellarLend devnet ready
============================================================
 RPC:            ${RPC_URL}
 Passphrase:     ${PASSPHRASE}
 Lending:        ${LENDING_ID}
 USDC / XLM:     ${USDC_ID} / ${XLM_ID}
 Admin identity: ${ADMIN}   (stellar keys address ${ADMIN})
 Users:          ${ALICE}, ${BOB}, ${CAROL}
 State file:     ${STATE_FILE}

 Try:  stellar contract invoke --id ${LENDING_ID} --source ${ALICE} \\
         --rpc-url ${RPC_URL} --network-passphrase '${PASSPHRASE}' \\
         --send=no -- get_user_position --user ${ALICE_ADDR}
============================================================
EOF
}

main() {
  preflight
  [ "${RESET}" -eq 1 ] && reset_devnet


  if [ "${DRY_RUN}" -eq 0 ] && [ -f "${STATE_FILE}" ] && rpc_healthy; then
    LENDING_ID="$(state_field lending)"; USDC_ID="$(state_field usdc)"; XLM_ID="$(state_field xlm)"
    ALICE_ADDR="$(stellar keys address "${ALICE}")"
    log "Devnet already bootstrapped (${STATE_FILE}). Use --reset to start over."
    summary
    return 0
  fi

  start_node

  ADMIN_ADDR="$(ensure_identity "${ADMIN}")"
  ALICE_ADDR="$(ensure_identity "${ALICE}")"
  BOB_ADDR="$(ensure_identity "${BOB}")"
  CAROL_ADDR="$(ensure_identity "${CAROL}")"

  build_contract
  local hash
  hash="$(wasm_hash)"
  LENDING_ID="$(deploy_lending)"
  XLM_ID="$(asset_contract_id native "<xlm-contract-id>")"
  USDC_ID="$(deploy_usdc)"

  seed
  verify
  write_state "${hash}"
  summary
}

main
