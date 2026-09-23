#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
E2E_DIR="${REPO_ROOT}/tests/e2e"

MODE="${1:-help}"

print_usage() {
    echo "Usage: $0 <command> [args]"
    echo ""
    echo "Commands:"
    echo "  run                Run the concurrent interaction simulation tests"
    echo "  stress <users>     Run stress test with N concurrent users (default: 50)"
    echo "  help               Show this help message"
}

run_concurrency_tests() {
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Multi-User Concurrent Interaction Simulation Tests"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    if [ ! -d "$E2E_DIR" ]; then
        echo "ERROR: tests/e2e directory not found at ${E2E_DIR}"
        exit 1
    fi

    cd "${E2E_DIR}"

    if [ ! -f "package.json" ]; then
        echo "ERROR: No package.json found in tests/e2e/"
        exit 1
    fi

    if [ ! -d "node_modules" ]; then
        echo "[1/2] Installing dependencies..."
        npm ci --silent 2>&1
        echo "      Install complete."
    else
        echo "[1/2] Dependencies already installed."
    fi

    echo "[2/2] Running concurrency tests..."
    npx jest concurrency.e2e.test.ts --verbose 2>&1

    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Concurrency tests complete."
    echo "═══════════════════════════════════════════════════════════════"
}

run_stress_test() {
    local num_users="${1:-50}"

    echo "═══════════════════════════════════════════════════════════════"
    echo "  Concurrent Interaction Stress Test — ${num_users} users"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    if [ ! -d "$E2E_DIR" ]; then
        echo "ERROR: tests/e2e directory not found at ${E2E_DIR}"
        exit 1
    fi

    cd "${E2E_DIR}"

    if [ ! -d "node_modules" ]; then
        echo "[1/3] Installing dependencies..."
        npm ci --silent 2>&1
    else
        echo "[1/3] Dependencies already installed."
    fi

    echo "[2/3] Running stress test with ${num_users} concurrent users..."
    echo "      (Simulating ${num_users} users performing simultaneous operations)"
    echo ""

    CONCURRENT_USERS="${num_users}" npx jest concurrency.e2e.test.ts --verbose --testNamePattern="Concurrent" 2>&1

    echo ""
    echo "[3/3] Stress test complete."
    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Stress test results: ${num_users} concurrent users simulated."
    echo "═══════════════════════════════════════════════════════════════"
}

case "$MODE" in
    run)
        run_concurrency_tests
        ;;
    stress)
        run_stress_test "${2:-50}"
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
