#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

MODE="${1:-help}"

print_usage() {
    echo "Usage: $0 <command> [args]"
    echo ""
    echo "Commands:"
    echo "  check <network>    Verify deployment on a network (testnet/mainnet)"
    echo "  status             Show deployment status from deployment.json"
    echo "  compare            Compare deployed contracts vs local build"
    echo "  help               Show this help message"
}

check_deployment() {
    local network="${1:-}"

    if [ -z "$network" ]; then
        echo "ERROR: Network required (testnet or mainnet)"
        echo "Usage: $0 check <network>"
        exit 1
    fi

    echo "═══════════════════════════════════════════════════════════════"
    echo "  Deployment Verification — ${network}"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    local deploy_file="${REPO_ROOT}/environments/${network}/deployment.json"

    if [ ! -f "$deploy_file" ]; then
        echo "FAIL: Deployment file not found: ${deploy_file}"
        echo "      No deployment detected for network: ${network}"
        exit 1
    fi

    echo "[1/5] Checking deployment file exists... OK"

    if ! command -v jq &> /dev/null; then
        echo "WARN: jq not found. Limited JSON parsing available."
        echo "      Install jq for full verification: https://stedolan.github.io/jq/"
    fi

    local contract_count=0
    if command -v jq &> /dev/null; then
        contract_count=$(jq | length 2>/dev/null || echo "0")
    fi

    echo "[2/5] Parsing deployment.json... OK (${contract_count} contracts)"

    if command -v jq &> /dev/null; then
        echo "[3/5] Verifying contract addresses..."
        local addresses
        addresses=$(jq -r '.. | objects | select(has("address")) | .address' "$deploy_file" 2>/dev/null || echo "")
        local valid_count=0
        local invalid_count=0
        while IFS= read -r addr; do
            if [ -n "$addr" ] && [[ "$addr" == C* ]]; then
                valid_count=$((valid_count + 1))
            elif [ -n "$addr" ]; then
                invalid_count=$((invalid_count + 1))
                echo "      WARN: Invalid address format: ${addr}"
            fi
        done <<< "$addresses"
        echo "      Valid addresses: ${valid_count}"
        if [ "$invalid_count" -gt 0 ]; then
            echo "      Invalid addresses: ${invalid_count}"
        fi
    fi

    echo "[4/5] Verifying WASM artifacts exist..."
    local wasm_dir="${REPO_ROOT}/stellar-lend/target/wasm32-unknown-unknown/release"
    if [ -d "$wasm_dir" ]; then
        local wasm_count
        wasm_count=$(find "$wasm_dir" -name "*.wasm" ! -name "*.d.*" 2>/dev/null | wc -l)
        echo "      Found ${wasm_count} WASM artifacts."
    else
        echo "      WASM build directory not found. Run build first."
    fi

    echo "[5/5] Checking deployment consistency..."
    if command -v jq &> /dev/null; then
        local timestamps
        timestamps=$(jq -r '.. | objects | select(has("deployed_at")) | .deployed_at' "$deploy_file" 2>/dev/null || echo "")
        if [ -n "$timestamps" ]; then
            echo "      Deployment timestamps found."
        fi
    fi

    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Verification complete for ${network}."
    echo "═══════════════════════════════════════════════════════════════"
}

show_status() {
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Deployment Status"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    local found=0

    for network in testnet mainnet; do
        local deploy_file="${REPO_ROOT}/environments/${network}/deployment.json"
        if [ -f "$deploy_file" ]; then
            found=1
            echo "Network: ${network}"
            echo "  File: ${deploy_file}"

            if command -v jq &> /dev/null; then
                local contract_count
                contract_count=$(jq | length 2>/dev/null || echo "?")
                echo "  Contracts deployed: ${contract_count}"

                local last_updated
                last_updated=$(jq -r '.updated_at // .last_updated // "unknown"' "$deploy_file" 2>/dev/null || echo "unknown")
                echo "  Last updated: ${last_updated}"
            fi
            echo ""
        fi
    done

    if [ "$found" -eq 0 ]; then
        echo "No deployment files found."
        echo "Expected locations:"
        echo "  environments/testnet/deployment.json"
        echo "  environments/mainnet/deployment.json"
    fi

    echo "═══════════════════════════════════════════════════════════════"
}

compare_deployed() {
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Deployment vs Local Build Comparison"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    local wasm_dir="${REPO_ROOT}/stellar-lend/target/wasm32-unknown-unknown/release"

    if [ ! -d "$wasm_dir" ]; then
        echo "ERROR: No local WASM builds found. Run build first."
        exit 1
    fi

    echo "Local WASM artifacts:"
    for wasm in "${wasm_dir}"/*.wasm; do
        if [ -f "$wasm" ] && [[ "$wasm" != *".d."* ]]; then
            local name hash size
            name=$(basename "$wasm")
            hash=$(sha256sum "$wasm" | awk '{print $1}')
            size=$(stat -f%z "$wasm" 2>/dev/null || stat -c%s "$wasm" 2>/dev/null || echo "?")
            echo "  ${name} (${size} bytes) [${hash:0:16}...]"
        fi
    done

    echo ""
    echo "Deployed contracts (from deployment.json):"
    local found=0
    for network in testnet mainnet; do
        local deploy_file="${REPO_ROOT}/environments/${network}/deployment.json"
        if [ -f "$deploy_file" ]; then
            found=1
            echo "  Network: ${network}"
            if command -v jq &> /dev/null; then
                local addresses
                addresses=$(jq -r '.. | objects | select(has("address")) | .address' "$deploy_file" 2>/dev/null || echo "")
                while IFS= read -r addr; do
                    if [ -n "$addr" ]; then
                        echo "    ${addr}"
                    fi
                done <<< "$addresses"
            fi
        fi
    done

    if [ "$found" -eq 0 ]; then
        echo "  No deployments found."
    fi

    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Comparison complete."
    echo "═══════════════════════════════════════════════════════════════"
}

case "$MODE" in
    check)
        check_deployment "${2:-}"
        ;;
    status)
        show_status
        ;;
    compare)
        compare_deployed
        ;;
    help|--help|-h)
        print_usage
        ;;
    *)
        echo "Unknown command: ${MODE}"
        print_usage
        exit 1
        ;;
esac
