# Architectural Decision Records (ADRs)

This directory contains records of the major architectural and design decisions made in StellarLend.

## What is an ADR?
An Architectural Decision Record (ADR) captures a single architecture decision made for StellarLend along with its context and consequences.

## Index of ADRs

| Number | Title | Status | Date | PR |
| :--- | :--- | :--- | :--- | :--- |
| [ADR-0001](0001-record-architecture-decisions.md) | Record Architecture Decisions | Accepted | 2026-11-25 | #1788 |

## Creating a New ADR
1. Copy [template.md](template.md) to `docs/adr/[NUMBER]-[title-in-kebab-case].md` (format: 4-digit zero-padded number, e.g. `0002-liquidator-incentive-curve.md`).
2. Fill out all sections.
3. Update this index table.
4. Submit a PR. CI validation will verify header format, sequential numbering, and status transitions.
