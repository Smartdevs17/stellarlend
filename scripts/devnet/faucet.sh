#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# faucet.sh -- fund devnet accounts with XLM and devnet USDC in one command
#
#   npm run devnet                                  # once, to create the devnet
#   npm run devnet:faucet -- sl-dave                # new or existing identity
#   npm run devnet:faucet -- GABC... --usdc 500     # any account or contract
#
# For each target:
# 1. Identity names that don't exist yet are generated (`stellar keys generate`).
# 2. Accounts (G...) get XLM from friendbot.
# 3. Identities get a USDC trustline (signed with their own key).
# 4. The devnet admin mints USDC to the target through the USDC asset contract.
#
# Raw G... addresses can't be given a trustline here (we don't hold their key), so
# they must already trust USDC:<admin>; contract (C...) targets need no trustline.
#
# Docs: docs/devnet.md#faucet
# ---------------------------------------------------------------------------
set -euo pipefail

# shellcheck source=scripts/devnet/lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

USDC_AMOUNT=10000
FUND_XLM=1
TARGETS=()

usage() {
  cat <<EOF
Usage: $(basename "$0") [options] <identity|address>...

  --usdc <amount>  Whole USDC to mint to each target (default ${USDC_AMOUNT}; 0 skips USDC)
  --no-xlm         Skip friendbot XLM funding
  --dry-run        Print every command instead of running it
  -h, --help       Show this help

Targets are stellar CLI identity names (created if missing), G... accounts or C...
contracts. Reads contract ids from \$DEVNET_STATE_DIR/state.json (written by
'npm run devnet'); honours the same DEVNET_* overrides as bootstrap.sh.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --usdc) USDC_AMOUNT="${2:?--usdc needs an amount}"; shift ;;
    --no-xlm) FUND_XLM=0 ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help) usage; exit 0 ;;
    -*) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
    *) TARGETS+=("$1") ;;
  esac
  shift
done

[ "${#TARGETS[@]}" -gt 0 ] || { usage >&2; exit 2; }
[[ "${USDC_AMOUNT}" =~ ^[0-9]+$ ]] || die "--usdc must be a whole, non-negative number (got '${USDC_AMOUNT}')"

load_state() {
  if [ "${DRY_RUN}" -eq 1 ] && [ ! -f "${STATE_FILE}" ]; then
    USDC_ID="<usdc-contract-id>"
    ADMIN_ADDR="<${ADMIN}-address>"
    return 0
  fi
  [ -f "${STATE_FILE}" ] || die "${STATE_FILE} not found; run 'npm run devnet' first"
  USDC_ID="$(state_field usdc)"
  # The admin entry is the first "address" in the accounts block.
  ADMIN_ADDR="$(state_field address)"
  [ -n "${USDC_ID}" ] && [ -n "${ADMIN_ADDR}" ] || die "${STATE_FILE} is missing the usdc contract or admin address"
  if [ "${DRY_RUN}" -eq 0 ]; then
    command -v stellar >/dev/null || die "stellar CLI not found. Install: cargo install --locked stellar-cli"
    rpc_healthy || die "RPC at ${RPC_URL} is not healthy; is the devnet running? (npm run devnet)"
  fi
}

# Print the address for a target, generating the identity if it doesn't exist yet.
resolve() {
  local target="$1"
  if is_address "${target}"; then
    echo "${target}"
    return 0
  fi
  if [ "${DRY_RUN}" -eq 1 ] || ! stellar keys address "${target}" >/dev/null 2>&1; then
    log "Creating identity ${target}..."
    run stellar keys generate "${target}" >/dev/null
  fi
  capture "<${target}-address>" stellar keys address "${target}"
}

fund() {
  local target="$1" addr
  addr="$(resolve "${target}")"

  if [ "${FUND_XLM}" -eq 1 ] && [[ "${addr}" != C* ]]; then
    friendbot_fund "${addr}" "${target}"
  fi

  if [ "${USDC_AMOUNT}" -gt 0 ]; then
    mint_usdc "${target}" "${addr}"
  fi
  echo "${target} ${addr}"
}

mint_usdc() {
  local target="$1" addr="$2"
  if ! is_address "${target}"; then
    log "Adding USDC trustline for ${target}..."
    # Re-adding an existing trustline is a no-op that still succeeds.
    run stellar tx new change-trust --source "${target}" --line "USDC:${ADMIN_ADDR}" "${NET_ARGS[@]}" >/dev/null
  fi
  log "Minting ${USDC_AMOUNT} USDC to ${target}..."
  run stellar contract invoke --id "${USDC_ID}" --source "${ADMIN}" "${NET_ARGS[@]}" -- \
    mint --to "${addr}" --amount "$((USDC_AMOUNT * UNIT))" >/dev/null \
    || die "mint to ${target} failed (a raw G... address must already trust USDC:${ADMIN_ADDR})"
}

load_state
for target in "${TARGETS[@]}"; do fund "${target}"; done
