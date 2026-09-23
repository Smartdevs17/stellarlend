# Formal Verification Specifications for Core Lending Invariants

## Overview

This directory contains formal verification specifications for the StellarLend core lending protocol using the [Certora Prover](https://www.certora.com/) framework.

Extended for **Issue #687** with health-factor bounds and interest-accrual invariant specs, a verification report script, and a CI structure-check gate.

## Invariants

### Core lending (`core_lending_invariants.spec`)

| ID | Name | Description |
|----|------|-------------|
| INV-001 | Per-User Solvency | Health factor ≥ 1.0 for all users with debt |
| INV-002 | Collateral Non-Negative | No user can have negative collateral balance |
| INV-003 | Debt Non-Negative | No user can have negative debt balance |
| INV-004 | Liquidation Eligibility | Liquidatable users have both collateral and debt |
| INV-005 | No Value Creation | Borrowing cannot create value from nothing |
| INV-006 | Admin Stability | Admin address is consistent and non-zero |
| INV-007 | Pause Immutability | Balances unchanged during pause |
| INV-008 | Health Factor Consistency | Health factor consistent with underlying values |
| INV-009 | Collateral Covers Debt | Healthy positions have sufficient coverage |
| INV-010 | Total Assets Monotonicity | Total assets never become negative |
| INV-011 | No Mint on Borrow | Borrow does not inflate total supply |
| INV-012 | Interest Index Monotonicity | Interest index never decreases |
| INV-013 | Reserve Monotonicity | Reserves never become negative |
| INV-014 | Access Control | Admin functions revert for non-admin callers |

### Health factor bounds (`health_factor_bounds.spec`) — Issue #687

| ID | Name |
|----|------|
| HF-001 | Zero debt → HF ≥ 1.0 |
| HF-002 | Positive debt → HF > 0 |
| HF-003 | HF consistency with collateral/debt/threshold (±1) |
| HF-004 | Monotonic in collateral |
| HF-005 | Monotonic in debt |
| HF-006 | Liquidatable state consistency |
| HF-007 | Pause non-negative collateral |

### Interest accrual (`interest_accrual.spec`) — Issue #687

| ID | Name |
|----|------|
| IA-001 | Interest index monotonicity |
| IA-002 | Zero rate → zero accrual |
| IA-003 | Reserves non-negative after accrual |
| IA-004 | Debt never decreases from accrual alone |
| IA-005 | Interest split (supplier + reserves) conserves total |
| IA-006 | Total assets non-negative after accrual |
| IA-007 | Same-timestamp accrual idempotent |
| IA-008 | Reserve factor bounds [0, 10000] |

### Interest rate model (`interest_rate_model.spec`)

| ID | Name |
|----|------|
| IRM-001…IRM-008 | Boundary conditions, monotonicity, kink continuity, overflow safety |

### Oracle integration (`oracle_integration.spec`)

| ID | Name |
|----|------|
| ORA-001…ORA-010 | Price positivity, freeze behavior, feed health |

## Files

- `core_lending_invariants.spec` — CVL rules for all 14 core invariants
- `health_factor_bounds.spec` — HF-001…HF-007 (Issue #687)
- `interest_accrual.spec` — IA-001…IA-008 (Issue #687)
- `interest_rate_model.spec` — IRM-001…IRM-008
- `oracle_integration.spec` — ORA-001…ORA-010
- `config.json` — Certora Prover configuration (all five specs)
- `verification-baseline.json` — Structure-check baseline for CI

## Running

```bash
# Full Certora run (requires prover access)
certoraRun certora/config.json

# Local structure / report check (no prover needed)
bash scripts/verification-report.sh
```

## Structure Check (CI)

`scripts/verification-report.sh` verifies:

1. Every `.spec` file listed in `config.json` exists
2. Each spec contains `methods {` and at least one `rule `
3. Spec IDs match `verification-baseline.json`
4. Emits a markdown report artifact

## References

- [Certora Documentation](https://docs.certora.com/)
- [CVL Reference Manual](https://docs.certora.com/en/latest/docs/cvl/index.html)
- Issue #808: Implement formal verification specifications for core lending invariants
- Issue #687: Formal verification specs for core lending invariants (HF + IA)
