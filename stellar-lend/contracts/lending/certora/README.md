# Formal Verification Specifications for Core Lending Invariants

## Overview

This directory contains formal verification specifications for the StellarLend core lending protocol using the [Certora Prover](https://www.certora.com/) framework.

## Invariants

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

## Files

- `core_lending_invariants.spec` — CVL rules for all 14 invariants
- `config.json` — Certora Prover configuration

## Running

```bash
certoraRun certora/config.json
```

## References

- [Certora Documentation](https://docs.certora.com/)
- [CVL Reference Manual](https://docs.certora.com/en/latest/docs/cvl/index.html)
- Issue #808: Implement formal verification specifications for core lending invariants
