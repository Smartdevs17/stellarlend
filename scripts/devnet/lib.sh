#!/usr/bin/env bash
# Shared configuration and helpers for scripts/devnet/*.sh. Sourced, not executed.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

STATE_DIR="${DEVNET_STATE_DIR:-${ROOT}/.devnet}"
STATE_FILE="${STATE_DIR}/state.json"
RPC_URL="${DEVNET_RPC_URL:-http://localhost:8000/rpc}"
FRIENDBOT_URL="${DEVNET_FRIENDBOT_URL:-http://localhost:8000/friendbot}"
PASSPHRASE="${DEVNET_PASSPHRASE:-Standalone Network ; February 2017}"
CONTAINER="${DEVNET_CONTAINER:-local}"
KEY_PREFIX="${DEVNET_KEY_PREFIX:-sl}"
HEALTH_TIMEOUT="${DEVNET_HEALTH_TIMEOUT:-180}"
LENDING_MANIFEST="${ROOT}/stellar-lend/contracts/lending/Cargo.toml"
WASM="${DEVNET_WASM:-${ROOT}/stellar-lend/target/wasm32v1-none/release/stellarlend_lending.wasm}"
MIN_STELLAR_CLI="25.2.0"

# Amounts use 7 decimals (1 unit = 10^7 stroops), matching Stellar assets.
UNIT=10000000

DRY_RUN="${DRY_RUN:-0}"

log() { echo "[devnet] $*" >&2; }
die() { echo "[devnet] ERROR: $*" >&2; exit 1; }

NET_ARGS=(--rpc-url "${RPC_URL}" --network-passphrase "${PASSPHRASE}")

# Run a command, or print it under --dry-run.
run() {
  if [ "${DRY_RUN}" -eq 1 ]; then
    printf '+' >&2; printf ' %q' "$@" >&2; printf '\n' >&2
  else
    "$@"
  fi
}

# Run a command and print its stdout; under --dry-run print a placeholder instead.
capture() {
  local placeholder="$1"; shift
  if [ "${DRY_RUN}" -eq 1 ]; then
    run "$@"
    echo "${placeholder}"
  else
    "$@"
  fi
}

# Identity names derived from the key prefix.
ADMIN="${KEY_PREFIX}-admin"
ALICE="${KEY_PREFIX}-alice"
BOB="${KEY_PREFIX}-bob"
CAROL="${KEY_PREFIX}-carol"

version_ge() { [ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" = "$2" ]; }

rpc_healthy() {
  curl -sf -X POST "${RPC_URL}" -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' 2>/dev/null | grep -q '"healthy"'
}

state_field() {
  # Extract a "key": "value" string from the state file without requiring jq.
  grep -oE "\"$1\": \"[^\"]*\"" "${STATE_FILE}" | head -n1 | sed -E 's/.*: "(.*)"/\1/'
}

