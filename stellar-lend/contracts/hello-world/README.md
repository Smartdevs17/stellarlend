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
- Bribe market: `bribe_deposit`, `bribe_settle`, `bribe_claim`, `get_bribe_epoch`, `get_bribe_reward`
- Cross-chain collateral: `register_eth_attestation`, `wrap_eth_collateral`, `unwrap_eth_collateral`, `get_wrapped_collateral`
- Bulk repayment: `bulk_repay`, `get_bulk_repay_quote`
- ERC-4626 stablecoin vaults: `initialize_pool_vault`, `set_pool_vault_config`, `get_pool_vault_config`, `vault_asset`, `vault_total_assets`, `vault_total_shares`, `vault_convert_to_shares`, `vault_convert_to_assets`, `vault_max_deposit`, `vault_preview_deposit`, `vault_deposit`, `vault_max_mint`, `vault_preview_mint`, `vault_mint`, `vault_max_withdraw`, `vault_preview_withdraw`, `vault_withdraw`, `vault_max_redeem`, `vault_preview_redeem`, `vault_redeem`, `vault_share_balance_of`, `vault_share_allowance`, `vault_approve_shares`, `vault_transfer_shares`, `vault_accrue_yield`, `vault_get_stats`

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

## Bribe Market (`src/bribes.rs`)

The bribe market lets anyone pay voters to direct epoch-based gauge emissions. Bribes are
scoped to a `(gauge, epoch)` pair, so a deposit only ever rewards votes cast in that epoch.

### Lifecycle

1. **Deposit** — `bribe_deposit(caller, gauge, epoch, asset, amount)` transfers `amount` of
   `asset` from `caller` into the contract and records it against `(gauge, epoch)`. Deposits are
   only accepted for the current or a future epoch; depositing into a closed epoch returns
   `BribeError::EpochClosed`. A zero or negative amount returns `BribeError::InvalidAmount`.
2. **Settle** — `bribe_settle(caller, gauge, epoch)` is permissionless and callable only after the
   epoch has ended. It snapshots the epoch's total voting power and computes each voter's pro-rata
   share of every deposited asset. Settling twice returns `BribeError::AlreadySettled`; settling
   before the epoch ends returns `BribeError::EpochNotEnded`.
3. **Claim** — `bribe_claim(caller, gauge, epoch, asset)` pays the caller their settled share and
   zeroes it out. Claiming before settlement returns `BribeError::NotSettled`; claiming twice
   returns `BribeError::NothingToClaim`.

Read-only helpers: `get_bribe_epoch(gauge, epoch)` returns the epoch's deposit totals and
settlement status, and `get_bribe_reward(gauge, epoch, voter, asset)` returns a voter's
unclaimed share.

### Events emitted

| Topics | Payload | Trigger |
|---|---|---|
| `("bribe", "deposit")` | `(gauge, epoch, asset, amount)` | `bribe_deposit` |
| `("bribe", "settled")` | `(gauge, epoch, total_voting_power)` | `bribe_settle` |
| `("bribe", "claim")` | `(gauge, epoch, asset, voter, amount)` | `bribe_claim` |

Off-chain indexers subscribe to the `bribe` topic prefix to build the reward leaderboard.

---

## Cross-Chain Collateral Wrapper (`contracts/bridges/`)

The bridge wrapper lets assets held on Ethereum be used as collateral on Stellar without
custodying the underlying token on this chain.

### Flow

1. **Attest** — `register_eth_attestation(admin, eth_tx_hash, holder, asset, amount)` records a
   signed Ethereum deposit attestation. Only the configured bridge admin may call it; a repeated
   `eth_tx_hash` returns `BridgeError::AttestationReplayed`, which prevents double-minting from a
   replayed message. The attestation must be within the configured freshness window or the call
   returns `BridgeError::StaleAttestation`.
2. **Wrap** — `wrap_eth_collateral(holder, asset, amount)` mints a 1:1 wrapped representation
   against a previously registered attestation and credits it to the holder's collateral balance.
   Wrapping more than the attested amount returns `BridgeError::InsufficientAttestation`.
3. **Unwrap** — `unwrap_eth_collateral(holder, asset, amount)` burns the wrapped representation and
   emits a release request that the relayer settles on Ethereum. Unwrapping more than the holder's
   wrapped balance returns `BridgeError::InsufficientWrappedBalance`.

`get_wrapped_collateral(holder, asset)` returns the holder's current wrapped balance.

### API routes

The bridge service exposes the wrapper over HTTP:

| Method | Route | Description |
|---|---|---|
| `POST` | `/bridge/eth-collateral/attestations` | Submit an Ethereum deposit attestation |
| `GET` | `/bridge/eth-collateral/attestations/:tx_hash` | Fetch a recorded attestation |
| `POST` | `/bridge/eth-collateral/wrap` | Wrap attested collateral |
| `POST` | `/bridge/eth-collateral/unwrap` | Request an unwrap and Ethereum release |
| `GET` | `/bridge/eth-collateral/:holder/:asset` | Read a holder's wrapped balance |

### Events emitted

| Topics | Payload | Trigger |
|---|---|---|
| `("bridge", "attested")` | `(eth_tx_hash, holder, asset, amount)` | `register_eth_attestation` |
| `("bridge", "wrapped")` | `(holder, asset, amount)` | `wrap_eth_collateral` |
| `("bridge", "unwrap_requested")` | `(holder, asset, amount)` | `unwrap_eth_collateral` |

---

## Bulk Repayment (`src/repay.rs`)

`bulk_repay(caller, payments)` repays several debts in a single transaction. Each entry in
`payments` names an asset and an amount; the contract settles them in one pass so a user with
positions across multiple assets pays one base fee instead of one per asset.

### Cross-asset settlement

An entry may repay a debt denominated in a different asset than the one supplied. The contract
prices the supplied asset through the oracle (`get_price`) and converts it into the debt asset
before applying the payment. If the oracle has no price for either asset the call returns
`RepayError::PriceUnavailable`; if the supplied amount is worth less than one unit of the debt
asset the entry is skipped rather than reverting the whole batch.

### Gas amortisation

The batch charges a single base cost plus a per-entry marginal cost that is strictly smaller than
the standalone `repay_debt` cost, so larger batches are cheaper per repayment. `get_bulk_repay_quote(payments)`
returns the expected total cost and the per-entry breakdown without mutating state, letting clients
size a batch before submitting it. An empty batch returns `RepayError::EmptyBatch`; a batch larger
than the configured maximum returns `RepayError::BatchTooLarge`.

### Events emitted

| Topics | Payload | Trigger |
|---|---|---|
| `("repay", "bulk")` | `(caller, entries, total_repaid, assets)` | `bulk_repay` |

The aggregated `("repay", "bulk")` event is emitted once per batch rather than once per entry, so
indexers see a single record for the whole repayment.

---

Current note:

- `HelloContract` does not currently expose dedicated upgrade entrypoints from `src/lib.rs`.
- The separate upgrade approval manager documented in [`../../../docs/upgrade-mechanism.md`](../../../docs/upgrade-mechanism.md) lives under `stellar-lend/contracts/lending`.

Refer to `src/lib.rs` for the current public entrypoints and to [`../../../docs/event-indexing.md`](../../../docs/event-indexing.md) for event topics and indexing guidance.

## Governance Module Refactoring (`src/governance/`)

### Structure Improvements

The governance module has been refactored to eliminate duplication and clarify intent:

**New centralized state module** (`src/governance/state.rs`):
- Single source of truth for all storage access patterns
- Functions: `get_config`, `load_proposal`, `save_proposal`, `next_proposal_id`, `get_admin`, `get_multisig_config`, `get_proposal_approvals`, `save_proposal_approvals`
- All errors use consistent patterns (`NotInitialized` for missing config/admin/multisig, `ProposalNotFound` for missing proposals)

**Eliminated duplication**:
- Removed duplicate `get_config` implementations from `proposal.rs` and `power.rs`
- Removed 80+ lines of bare event emitters from `mod.rs` (now uses type-safe events)
- All modules now use state module for storage operations
- ~60 net lines of code eliminated

**Testability**:
- 14 unit tests in state module cover success and error paths
- Each storage function tested in isolation
- Tests verify correct behavior on initialization, overwrite, and missing data

See [`docs/GOVERNANCE_REFACTORING.md`](docs/GOVERNANCE_REFACTORING.md) for detailed migration guide and design rationale.

---

## ERC-4626 Tokenized Vault Interface for Stablecoin Pools (`src/vault.rs`)

### Overview & Motivation (#1608)

Stablecoin pools (such as USDC, EURC, USDS) often require a clean, standardized abstraction to represent yield-bearing deposits. Previously, pool logic and share tracking were ad-hoc, duplicating conversion math and limit checks across multiple contracts.

The `vault` module extracts a canonical **ERC-4626 Tokenized Vault Interface** implemented natively in Soroban. It wraps pool reserves into standard share accounting and provides:

1. **Standard Trait (`ERC4626Vault`)**: Standardized methods (`asset`, `total_assets`, `total_shares`, `convert_to_shares`, `convert_to_assets`, `max_deposit`, `preview_deposit`, `deposit`, `max_mint`, `preview_mint`, `mint`, `max_withdraw`, `preview_withdraw`, `withdraw`, `max_redeem`, `preview_redeem`, `redeem`).
2. **Virtual Offset Inflation Protection**: Mitigates the first-depositor inflation / donation attack by employing virtual offsets (`VIRTUAL_OFFSET = 1`) on total assets and total shares.
3. **ERC-4626 Compliant Rounding**:
   - `convert_to_shares`, `preview_deposit`, and `preview_redeem` round **down** (in favor of vault).
   - `preview_mint` and `preview_withdraw` round **up** (in favor of vault).
4. **Third-Party Delegations**: Standard allowance approvals (`approve_shares`, `share_allowance`) allowing routers and custodians to execute withdrawals and redemptions on behalf of vault share owners.
5. **Multi-Pool Isolation**: Parameterized by `pool: Option<Address>` (`None` for default pool, `Some(address)` for individual stablecoin pools).

### Events Emitted

| Event | Topics | Payload |
|---|---|---|
| `VaultDepositEvent` | `("vault", "deposit")` | `pool, caller, receiver, assets, shares, timestamp` |
| `VaultWithdrawEvent` | `("vault", "withdraw")` | `pool, caller, receiver, owner, assets, shares, timestamp` |
| `VaultConfiguredEvent` | `("vault", "configured")` | `pool, caller, deposit_cap, deposit_paused, withdraw_paused, timestamp` |
| `VaultApprovalEvent` | `("vault", "approval")` | `pool, owner, spender, amount, timestamp` |
| `VaultYieldAccruedEvent` | `("vault", "yield_accrued")` | `pool, yield_amount, new_total_assets, timestamp` |

