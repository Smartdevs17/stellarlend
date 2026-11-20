# Hello-World Contract (StellarLend Core)

This contract exposes the current public API for the main StellarLend protocol contract.

## Key Entry Points

- Initialization: `initialize(admin)`, `transfer_admin(caller, new_admin)`
- Governance setup: `gov_initialize(...)`
- Core lending: `deposit_collateral`, `borrow_asset`, `repay_debt`, `withdraw_collateral`, `liquidate`
- Risk controls: `set_risk_params`, `set_emergency_pause`, `can_be_liquidated`, `get_max_liquidatable_amount`, `get_liquidation_incentive_amount`, `require_min_collateral_ratio`
- Flash loans: `execute_flash_loan`, `repay_flash_loan`, `configure_flash_loan`
- MEV protection: `commit_*_protected`, `commit_*_guarded`, `open_liq_batch_auction`, `submit_liq_batch_bid`, `settle_liq_batch_auction`, `register_private_mev_route`, `record_mev_gas_bid`, `get_mev_dashboard`
- Treasury: `set_treasury`, `get_treasury`, `get_reserve_balance`, `claim_reserves`, `set_fee_config`, `get_fee_config`
- Analytics and queries: `get_protocol_stats`, `get_protocol_report`, `get_user_position`, `get_user_report`, `get_recent_activity`, `get_user_asset_collateral`, `get_user_asset_list`, `get_user_total_collateral_value`, `get_health_factor`
- Asset configuration: `update_asset_config`

---

## Security Hardening — Timelock (`src/timelock.rs`)

### Gap closed

Two integrity gaps were present in the timelock:

**Re-initialization attack.** `initialize_timelock` had no idempotency guard. A privileged caller could call it again after the protocol was live, resetting the proposal counter to zero. This would silently render all existing pending proposals unreachable by id — a denial-of-governance condition. The fix adds a persistent sentinel on the `Delay` storage key: once set, any subsequent call returns `TimelockError::AlreadyInitialized` before any other check runs, so even the legitimate admin cannot reset the counter.

**Ticket replay in `update_delay`.** The original scan walked all proposals for any matching executed `update_delay` proposal whose `eta` had passed. A single execution ticket could therefore satisfy an unlimited number of subsequent `update_delay` calls — an insufficiently scoped privileged action. The fix stores the id of the most recently consumed ticket in `LastDelayProposalId`. The scan skips any proposal whose id matches the stored value, ensuring each executed proposal satisfies at most one delay update.

**Missing observability.** No events were emitted for any state change. The fix adds `("timelock", <action>)` events for every mutation: `initialized`, `proposed`, `executed`, `cancelled`, `delay_updated`. Off-chain indexers and monitoring subscribe to the `timelock` topic prefix.

### Error variants added

| Variant | Condition |
|---|---|
| `AlreadyInitialized` | `initialize_timelock` called after delay was already stored |

### Events emitted

| Topics | Payload | Trigger |
|---|---|---|
| `("timelock", "initialized")` | `delay: u64` | First successful `initialize_timelock` |
| `("timelock", "proposed", fn_name)` | `(id, eta)` | `propose` |
| `("timelock", "executed")` | `id` | `execute_proposal` |
| `("timelock", "cancelled")` | `id` | `cancel_proposal` |
| `("timelock", "delay_updated")` | `(old, new, ticket_id)` | `update_delay` |

`ticket_id` is `u64::MAX` for the bootstrap path (no proposal consumed).

---

## Security Hardening — Safe Math (`src/math.rs`)

### Gap closed

The contract previously used `safe_math.rs`, a minimal stub whose `checked_div` forwarded directly to Rust's `i128::checked_div`. That function returns `None` for `i128::MIN / -1` — the unique overflow case in integer division — but the stub surfaced that as a generic `None` with no distinction from division by zero. It also had no tests and no observability.

`math.rs` replaces the stub with a production-ready module that:

- **Guards both division failure modes explicitly**: `safe_div` checks `b == 0` first (emits `div_zero` event, returns `DivisionByZero`) and then calls `checked_div`, mapping `None` to `Overflow` (the `i128::MIN / -1` case).
- **Covers `u128` variants**: `safe_add_u128`, `safe_sub_u128`, `safe_mul_u128`, `safe_div_u128`.
- **Provides composite helpers**: `bps_of`, `ratio_bps`, `checked_mul_div` (with split-decomposition fallback for wide products).
- **Emits observability events**: every error path publishes a `("math", "overflow")`, `("math", "div_zero")`, or `("math", "neg_sqrt")` event when an `Env` is supplied. Pass `None` for pure-function callers.
- **Carries a full test suite**: 40+ unit tests covering boundary values, the `i128::MIN / -1` case, event emission on error, and table-driven cross-checks against `checked_*` primitives.

### Events emitted

| Topics | Trigger |
|---|---|
| `("math", "overflow")` | Overflow or underflow on any operation |
| `("math", "div_zero")` | Division by zero |
| `("math", "neg_sqrt")` | Square root of a negative number |

---

Current note:

- `HelloContract` does not currently expose dedicated upgrade entrypoints from `src/lib.rs`.
- The separate upgrade approval manager documented in [`../../../docs/upgrade-mechanism.md`](../../../docs/upgrade-mechanism.md) lives under `stellar-lend/contracts/lending`.

Refer to `src/lib.rs` for the current public entrypoints and to [`../../../docs/event-indexing.md`](../../../docs/event-indexing.md) for event topics and indexing guidance.
