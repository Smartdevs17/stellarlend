#!/usr/bin/env bash
#
# run_chaos.sh — Chaos experiment orchestrator for network failures (Issue #689).
#
# Executes the full chaos experiment cycle against the in-memory failure
# simulators and the E2E resilience suite:
#
#   1. inject     — run chaos suites (partition / RPC outage / degradation)
#   2. steady     — assert the system keeps serving reads under failure
#   3. recover    — stop failures and verify recovery procedures
#   4. report     — aggregate results into a markdown chaos report
#
# Subcommands:
#   run                 Full cycle (inject → steady → recover → report)
#   inject [suite]      Run one suite: partition | rpc | all (default all)
#   recover             Run only the recovery-oriented suites
#   report              Build report from the last JSON results
#   help                Show this help message
#
# Environment:
#   CHAOS_JSON          Path for raw Jest JSON results (default: tests/chaos/chaos-report.json)
#   CHAOS_MD            Path for markdown report (default: tests/chaos/chaos-report.md)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHAOS_DIR="${SCRIPT_DIR}/tests/chaos"
E2E_DIR="${SCRIPT_DIR}/tests/e2e"
CHAOS_JSON="${CHAOS_JSON:-${CHAOS_DIR}/chaos-report.json}"
CHAOS_MD="${CHAOS_MD:-${CHAOS_DIR}/chaos-report.md}"

MODE="${1:-help}"

print_usage() {
    echo "Usage: $0 <command> [args]"
    echo ""
    echo "Commands:"
    echo "  run                Full chaos cycle: inject, verify steady state, recover, report"
    echo "  inject [suite]     Inject failures only (suite: partition | rpc | all)"
    echo "  recover            Run recovery-procedure suites"
    echo "  report             Generate markdown report from last JSON results"
    echo "  help               Show this help message"
}

ensure_deps() {
    local dir="$1"
    if [ ! -f "${dir}/package.json" ]; then
        echo "ERROR: package.json not found in ${dir}"
        exit 1
    fi
    if [ ! -d "${dir}/node_modules" ]; then
        echo "      Installing dependencies in ${dir}..."
        (cd "${dir}" && npm ci --silent 2>&1)
    fi
}

run_jest_json() {
    # $1 = directory, remaining args = jest args
    local dir="$1"
    shift
    ensure_deps "${dir}"
    (cd "${dir}" && npx jest --runInBand --forceExit --json --outputFile="${CHAOS_JSON}" "$@")
}

build_report() {
    bash "${SCRIPT_DIR}/scripts/fuzz/chaos_report.sh" "${CHAOS_JSON}" "${CHAOS_MD}"
}

cmd_inject() {
    local suite="${1:-all}"
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Chaos Injection — suite: ${suite}"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    case "${suite}" in
        partition)
            run_jest_json "${CHAOS_DIR}" network-partition.test.ts
            ;;
        rpc)
            run_jest_json "${CHAOS_DIR}" network-failures.test.ts
            ;;
        all)
            run_jest_json "${CHAOS_DIR}"
            ;;
        *)
            echo "Unknown suite: ${suite} (expected partition | rpc | all)"
            exit 2
            ;;
    esac
}

cmd_recover() {
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Recovery Procedure Verification"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""
    echo "[1/2] Recovery suites in tests/chaos (recovery describe blocks)..."
    run_jest_json "${CHAOS_DIR}" --testNamePattern="Recovery|recovery"
    echo "[2/2] Graceful degradation + recovery in tests/e2e..."
    ensure_deps "${E2E_DIR}"
    (cd "${E2E_DIR}" && npx jest --runInBand --forceExit --testNamePattern="Recovery|degradation|outage" chaos-engineering.e2e.test.ts 2>&1)
    echo ""
    echo "Recovery procedures verified."
}

cmd_run() {
    echo "═══════════════════════════════════════════════════════════════"
    echo "  StellarLend Chaos Engineering Cycle (Issue #689)"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""
    echo "[1/4] Inject: network partition simulation..."
    run_jest_json "${CHAOS_DIR}" network-partition.test.ts
    echo ""
    echo "[2/4] Inject: RPC outage + graceful degradation..."
    run_jest_json "${CHAOS_DIR}" network-failures.test.ts
    echo ""
    echo "[3/4] Steady state + recovery: E2E resilience suite..."
    ensure_deps "${E2E_DIR}"
    (cd "${E2E_DIR}" && npx jest --runInBand --forceExit --json --outputFile=e2e-chaos-report.json chaos-engineering.e2e.test.ts 2>&1)
    echo ""
    echo "[4/4] Report: aggregating chaos results..."
    build_report
    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Chaos cycle complete. Report: ${CHAOS_MD}"
    echo "═══════════════════════════════════════════════════════════════"
}

case "$MODE" in
    run)      cmd_run ;;
    inject)   cmd_inject "${2:-all}" ;;
    recover)  cmd_recover ;;
    report)   build_report ;;
    help|--help|-h) print_usage ;;
    *)
        echo "Unknown command: ${MODE}"
        print_usage
        exit 1
        ;;
esac
