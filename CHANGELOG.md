# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project adheres to Semantic Versioning.

---

## [Unreleased]

### Added
- Lending pool gas cost estimator in `scripts/lending-gas-estimator`: statically indexes each contract entry point's storage accesses (including transitive calls, RAII guards, loop-bounded writes and the `#[contracttype]` size of every written value), prices the footprint in stroops using the API's own cost constants, and suggests optimizations from nine storage-pattern rules that each cite `file.rs:line`; includes a drift table diffing the API's hand-maintained `OPERATION_COMPLEXITY` against the contract source (#1011)
- Lending protocol gas budget planner in `scripts/gas-budget-planner`: a lender declares an interaction pattern (calls per period, optional batch size, optional budget) and the tool prices it per call, per period and annualised, projects it forward, and suggests six levers derived from the plan's own shape; per-call costs are read from `api/src/services/gas/estimator.ts` and the committed instruction budgets from `stellar-lend/benchmarks/baseline.json`, so a plan matches `POST /api/gas/estimate` instead of restating it (#1012)
- Lending pool position health simulation in `scripts/position-health-sim`: a faithful reference implementation of the contract's `compute_health_factor` (including its two truncating divisions, the no-debt sentinel, the no-oracle 0 and the overflow fallbacks), with the contract's constants read out of the Rust source rather than copied; simulates the committed `scenarios/*.json` corpus and price shock grids, computes the break-even collateral price and the distance to liquidation, and sweeps the admin-settable `liquidation_threshold_bps` to show what a change does to the liquidation boundary (#1013)
- Gas golf competition harness and leaderboard in `scripts/gas-golf`: a maintainer-owned differential correctness gate at tolerance 0, a `utilizationPct` leaderboard over `public-functions.json` targets, measurement-integrity checks, and an honest empty state where no `lending::*` measurement exists (#1014)
- Flash loan pool rules in the `flash-loan` crate: validated fee (rounded up) and min/max amount, a per-loan pool-share cap, balance-based repayment checking and metrics (#1019)
- Collateral factor registry in `lending-risk`: risk tiers with parameter ceilings, threshold-gap and coverage validation, freezing and supply caps (#1020)
- Liquidator loyalty incentives and an MEV guard (per-position cooldown, per-ledger cap, price-deviation band) in `lending-risk` (#1021)
- Documentation for the utilization-based variable interest rate model in `lending-interest` (#1018)
- Heartbeat monitoring in the Oracle Hub: opt-in per-asset reporting-cadence expectations (`interval_seconds`, `stale_after_seconds`, `expiry_seconds`), a permissionless idempotent sweep, and a fail-closed expiry that withholds a price nobody is updating instead of serving a stale one (#1034)
- Bounded price-history retention in the Oracle Hub: an opt-in fixed ring of resolved prices per asset, recorded on fresh resolutions only, queryable by range and recorded with the inputs that produced each decision (#1041)
- Governance-funded reporter incentives in the Oracle Hub: a token reward pool that pays oracle addresses for accepted reports, with per-asset rates, a per-oracle minimum interval, a rate ceiling, reporter-authorized claims, and a withdrawal floor that cannot touch earnings already owed (#1042)
- Market-wide emergency pause in the lending contract, with a guardian/admin-controlled lifecycle, operation-specific gates, and a reversible pause state that survives protocol calls already in flight (#1031)
- Opt-in TTL price cache in the Oracle Hub with freshness clamping, epoch invalidation on governance changes, and read-cost regression tests (#1037)
- Per-asset aggregation parameters (deviation band, source floor) and an explicit switch to disable the deviation check in the Oracle Hub (#1035)
- Per-asset feed index so the price read cost stays proportional to the sources actually registered (#1037)
- Health, ring-buffer, and event-trail views on the TWAP oracle, plus a documented `force_record_price` escape hatch for genuine repricings (#1033)
- Anomaly detection for unusual transaction patterns (median/MAD liquidation outliers, velocity bursts, amount spikes, dusting) in the analytics pipeline
- Multi-signature requirement for protocol upgrades (Oracle Hub upgrade gate with approver threshold and 48 h timelock)
- Timelock enforcement for governance parameter changes (per-parameter minimums plus maximum cap and view helpers in parameter-store)
- Local Soroban devnet setup with Docker Compose (node, Horizon, Friendbot, seeded accounts/contracts)
- Multi-stage Dockerization for API and Oracle services
- OpenAPI/Swagger documentation for API
- Price staleness detection and alerting in oracle service
- Retry logic with exponential backoff for transaction submission
- Monorepo root package configuration
- Circuit breaker implementation for system stability
- LRU batch cache eviction mechanism
- E2E integration tests for Oracle–Contract–API pipeline
- Multi-asset collateral support
- WebSocket endpoint for real-time price updates
- Health factor and position query entrypoints
- Protocol fee collection and treasury management
- Stellar address validation and enhanced middleware support
- Protocol governance system
- Oracle contract updater

### Changed
- TWAP oracle now computes a true time-weighted average (price × elapsed seconds) instead of a mean over sample counts, and rejects manipulated observations on ingestion instead of blending them into the average (#1033)
- Oracle Hub aggregation now supports five feed slots per asset with median, weighted, and trimmed-mean strategies (#1036)
- Oracle Hub now demotes a leading source that deviates from the rest beyond the configured band, with a quorum rule, a source floor, and on-chain evidence (#1035)
- Oracle Hub pull quotes are normalized to a hub-wide canonical precision before aggregation (#1036)
- Refactored duplicated validation logic using factory functions
- Switched transaction polling to fixed interval strategy
- Improved project structure and test organization
- Expanded full lifecycle integration test coverage
- Enhanced oracle performance and rate-limit handling
- Updated CI pipeline to enforce blocking contract checks
- Added contributing documentation

### Fixed
- Gas golf `--require-fresh` no longer passes a report it cannot verify: rows naming no commit are refused alongside rows from several commits, because `BenchmarkReport` writes no `git_commit` and "no commit found" was reading as a single clean session on every real report; the empty-state disclosure now reads the same blocking-reason list the scorer uses instead of restating it (#1014)
- Repaid, withdrawn, and liquidated positions stay open while a market is paused, so the pause stops new risk instead of trapping users in it (#1031)
- Range-check overflow lint in the isolated-market validation path
- Borrow interest overflow error
- Integration test placeholders and inconsistencies
- Default debt asset initialization for borrow/repay
- Flash loan reentrancy locks and VM execution aborts
- Masked admin secret key in oracle logs and outputs
- Enforced HTTPS and HSTS security headers
- Strengthened JWT secret validation and removed insecure defaults
- Validated CONTRACT_ID environment variable on API startup
- Fixed Rust CI issues (formatting, clippy, tests)

---

## 📌 Notes

- The project is under active development and changes are tracked under the Unreleased section.
- A formal version release will be added once versioning is standardized across the project.
