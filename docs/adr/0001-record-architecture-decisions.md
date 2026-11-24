# ADR-0001: Record Architecture Decisions

- **Status:** Accepted
- **Date:** 2026-11-25
- **Authors:** Sine Scribe (@sinescribe)
- **Deciders:** Smartdevs Core Team
- **Consulted:** Architecture Working Group
- **Informed:** All Contributors

## Context and Problem Statement
As StellarLend scales in contract complexity, protocol invariants, and cross-contract interfaces, technical decisions must be documented transparently for contributors, security auditors, and community integrators.

## Decision Drivers
* Prevent architecture regression and repeated debates on resolved questions.
* Standardize documentation of risk models, oracle integrations, and liquidity mechanisms.
* Automate CI verification of architectural changes.

## Decision Outcome
We will use Architectural Decision Records (ADRs) based on Michael Nygard's format, stored under `docs/adr/` in markdown and validated by GitHub Actions in CI.
