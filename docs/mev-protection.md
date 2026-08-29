# Sandwich Attack Protection

This document describes the lending pool sandwich-attack protection implemented in
`contracts/hello-world/src/mev_protection.rs` (issue #725).

## Threat model

A sandwich attack targets a sensitive on-chain operation (`borrow`, `withdraw`,
`liquidate`). An adversarial relay/sequencer observes the user's transaction and:

1. executes a **front-run** that moves the price or debt state against the user,
2. lets the user's transaction land in the middle,
3. executes a **back-run** that captures the price drift.

On Stellar/Soroban the protection is layered rather than purely execution-order
based, so the module combines **sequence detection**, **slippage protection**,
**commit–reveal privacy**, **private mempool routes**, and **liquidation auctions**.

## Layers

### 1. Sandwich detection (analytics + logging)

`record_ordering_signal(...)` is invoked on every sensitive reveal. It keeps the
latest two observations to the sequence: `(op, asset) -> latest/previous`:

- a **suspicious sequence** is flagged when the same actor acts twice within the
  configured window (`suspicious_window_secs`) with another actor in between,
- a **sandwich alert** is raised when a front-run actor's amount is within
  `sandwich_threshold_bps` of the sandwiched amount.

Detected attacks are:

- persisted in an append-only **sandwich attack log**
  (`MevDataKey::SandwichLog`, bounded to the 200 most recent records), and
- published as a `SANDWICH_ATTACK_DETECTED` event
  (`topics=(Symbol("SANDWICH_ATTACK_DETECTED"), operation, asset)`).

### 2. Slippage protection

The commit-reveal flow (see below) binds every reveal to **output constraints**:

- `min_output_amount` — the signed-off minimum the user is willing to receive,
- `max_slippage_bps` (default 500 bps / 5%) — an automatic floor derived from the
  observed/expected price at commit time.

A reveal that would clear less than `min_output_amount.max(slippage_floor)` is
rejected (`SlippageExpired`/`SlippageExceeded`), so the user is never sandwiched
into a worse-than-agreed execution. See `create_guarded_commit` /
`reveal_liquidation_with_output`.

### 3. Private transaction submission (commit–reveal)

`create_commit` / `create_guarded_commit` store a hash commitment and reveal only
the commitment on-chain early. The underlying `borrow`/`withdraw`/`liquidate`
reveal happens later with output constraints, so the exact parameters are not
publicly visible ahead of execution. `execution_hint(...)` and
`requires_commit_reveal(...)` tell integrators when the protective flow is needed.

### 4. Private mempool routes

`register_private_route(...)` lets a user designate a trusted relay
(`PrivateMempoolRoute`) for a time-limited window.
`record_private_execution(...)` stores execution receipts and per-route stats
(`PrivateRouteStats`) so integrators can route sensitive reveals off the public
mempool sequence. `get_private_route(env, route_id)` exposes them for tooling.

### 5. Liquidation auctions

Unprofitable/malicious liquidation races are auctioned instead of raced:

1. `open_liquidation_auction(...)` starts a time-boxed auction for a borrower,
2. liquidators `submit_liquidation_bid(...)` with `max_fee_bps` and
   `min_collateral_out` (their own slippage protection),
3. `settle_liquidation_auction(...)` executes the best bid at
   `min_rebate_bps` (rebate caps to the winner).

This removes the "first-to-tx" race payout that sandwiches depend on.
`create_liquidation_auction_commit(...)` extends commit-reveal to bids.

## Monitoring & incident flow

| Surface | Entrypoint | Purpose |
| --- | --- | --- |
| Analytics counters | `get_monitoring_dashboard(op, asset, amount)` | Ordering/gas-bid/auction/private-route stats + suggested fee/hint |
| Attack log | `get_sandwich_attack_log()` | Full append-only record of detected sequences |
| Attack summary | `get_sandwich_report()` | Total, last-24h, last timestamp, aggregate alert counters |
| Gas-bid data | `record_gas_bid_sample(...)` / `get_gas_bid_stats(...)` | Sequencing cost signal for recommended fees |
| On-chain event | `SANDWICH_ATTACK_DETECTED` | Real-time ingest for off-chain watchdogs |

### Off-chain consumers

The workspace repository layout contains the contract, benchmarks, and scripts.
The API/frontend surfaces referenced by the issue
(`api/src/services/mev.service.ts`, `api/src/routes/mev.ts`, `frontend/`)
are maintained in the application repository, which is out of scope here. The
contract surface above is the stable, versioned contract for those consumers:

- `get_mev_sandwich_attack_log`, `get_mev_sandwich_report`,
  `get_mev_protection_config`, `get_monitoring_dashboard` are the read APIs;
- the `SANDWICH_ATTACK_DETECTED` event is the streaming/ingest API.

## Tuning

`MevProtectionConfig` (set by admin via `configure_mev_protection`):

| Field | Default | Meaning |
| --- | --- | --- |
| `sandwich_threshold_bps` | 500 | Amount-close tolerance for classifying a sandwich |
| `suspicious_window_secs` | 600 | Window for sequence/suspicious detection |
| `max_slippage_bps` | 500 | Per-commit slippage floor for guarded reveals |
| `base_protection_fee_bps` / `surge_protection_fee_bps` | 10/50 | Protection fee schedule |
| `min_auction_bid_rebate_bps` | 50 | Minimum liquidation-auction rebate |

`configure` validates all bounds (`InvalidConfig` otherwise).

## Cost/security notes

- The sandwich log is bounded (`MAX_SANDWICH_LOG = 200`) to keep write cost flat;
  oldest entries are dropped first.
- Event topics use fixed `Symbol`s; payloads are compact
  `#[contracttype]` structs, so logging does not create unbounded read/write cost.
- Detection is heuristic: false positives are logged and raise `sandwich_alerts`
  but never block legitimate users. Blocking/denial would need a governance
  escalation on top of these signals.