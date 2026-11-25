# Cross-Pool Collateral Migration with Deferred Interest

## Overview

In multi-pool lending architectures, borrowers frequently need to migrate collateral between pools (e.g., rebalancing between general liquidity pools, migrating to high-yield or isolated lending markets, or adopting upgraded pool instances).

Historically, migrating collateral required borrowers to completely pay down all accrued interest and principal debt upfront in the source pool, or risk an immediate liquidation event due to collateral depletion. For users with large or illiquid positions, this created capital inefficiency and friction.

The **Cross-Pool Collateral Migration with Deferred Interest** module (`stellar-lend/contracts/hello-world/src/migrate.rs`) addresses Issue #1536 by enabling seamless, atomic collateral transfers between pools while deferring accrued interest into a structured repayment position.

---

## Key Features

1. **Atomic Collateral Migration**:
   - Gross collateral is deducted from `source_pool` and net collateral (after protocol fee) is credited to `destination_pool` in a single transaction.
2. **Deferred Interest Accounting**:
   - Accrued interest on the source position is snapshot into a `DeferredInterestRecord` tied to the borrower and destination pool.
   - The user avoids upfront cash outlays or forced liquidation during migration.
3. **Solvency & Health Factor Safeguards**:
   - If the borrower maintains active debt in the source pool, the migration is rejected if remaining collateral would drop the source pool health factor below `1.0` (10,000 basis points).
   - Pre-flight `preview_migration` enables clients to simulate health factors and net receipts before submission.
4. **Grace Period & Deferral Deadlines**:
   - A configurable deferral window (`max_deferral_seconds`, defaulting to 7 days) ensures interest cannot be deferred indefinitely.
5. **Partial and Full Settlement**:
   - Borrowers or protocol keepers can settle deferred interest via `settle_deferred_interest`.
   - Partial payments reduce outstanding principal; when settled in full, the record transitions to `is_settled: true`.
6. **Observability & Analytics**:
   - Monotonic migration identifiers track every event.
   - Comprehensive events emitted for indexing: `MigrationInitiatedEvent`, `MigrationCompletedEvent`, `DeferredInterestSettledEvent`, `MigrationConfigUpdatedEvent`.
   - Running totals recorded for total volume, fees collected, interest deferred, and interest settled.

---

## State & Data Structures

### `MigrationConfig`

| Field | Type | Description |
| --- | --- | --- |
| `admin` | `Address` | Protocol administrator address |
| `min_migration_amount` | `i128` | Minimum collateral amount allowed per migration |
| `max_migration_amount` | `i128` | Maximum collateral amount allowed per migration |
| `fee_bps` | `i128` | Protocol migration fee in basis points (e.g. 5 bps = 0.05%) |
| `max_deferral_seconds` | `u64` | Maximum duration before deferred interest expires |
| `cooldown_seconds` | `u64` | Per-user cooldown period between consecutive migrations |
| `paused` | `bool` | Emergency pause switch |

### `MigrationRecord`

| Field | Type | Description |
| --- | --- | --- |
| `id` | `u64` | Monotonic migration identifier |
| `user` | `Address` | Borrower address |
| `source_pool` | `Option<Address>` | Origin pool identifier (`None` = native XLM) |
| `destination_pool` | `Option<Address>` | Destination pool identifier |
| `asset` | `Option<Address>` | Migrated asset address |
| `collateral_migrated` | `i128` | Gross collateral debited from source pool |
| `net_collateral_credited` | `i128` | Net collateral credited to destination pool |
| `protocol_fee` | `i128` | Deducted protocol fee |
| `deferred_interest` | `i128` | Accrued interest deferred |
| `initiated_at` | `u64` | Timestamp of migration |
| `deferral_deadline` | `u64` | Expiration timestamp for deferred interest settlement |
| `status` | `MigrationStatus` | Current migration status (`Completed`, `Settled`, etc.) |

---

## Error Codes

| Code | Variant | Reason |
| --- | --- | --- |
| `1000` | `Unauthorized` | Caller is not authorized or lacked admin permissions |
| `1001` | `InvalidAmount` | Migration or settlement amount is zero or negative |
| `1002` | `SamePool` | Source and destination pool addresses are identical |
| `1003` | `MigrationTooSmall` | Migration amount is below `min_migration_amount` |
| `1004` | `MigrationTooLarge` | Migration amount exceeds `max_migration_amount` |
| `1005` | `InsufficientCollateral` | Source pool lacks sufficient collateral balance |
| `1006` | `UnhealthySourcePosition` | Migration would leave source health factor below 1.0 (10,000 bps) |
| `1007` | `MigrationPaused` | Migration operations are paused |
| `1008` | `DeferredInterestNotFound` | No active deferred interest record for user/pool |
| `1009` | `DeferralPeriodExpired` | Settlement attempted past deferral deadline |
| `1010` | `AlreadySettled` | Deferred interest has already been fully paid |
| `1011` | `InsufficientSettlementAmount` | Payment amount is zero or invalid |
| `1012` | `Overflow` | Checked math overflow |
| `1013` | `InvalidFee` | Fee basis points exceed 10,000 |
| `1014` | `CooldownNotElapsed` | User attempted migration before cooldown elapsed |
| `1015` | `InvalidConfig` | Parameter limits or thresholds are inconsistent |

---

## Testing & Verification

Unit tests in `stellar-lend/contracts/hello-world/src/migrate.rs` cover:
- Successful migration of collateral with deferred interest snapshot.
- Partial and complete settlement of deferred interest positions.
- Rejection of unhealthy source positions (`UnhealthySourcePosition`).
- Rejection of identical source and destination pools (`SamePool`).
- Rejection of insufficient collateral balances (`InsufficientCollateral`).
- Boundary validation for `min_migration_amount` and `max_migration_amount`.
- Cooldown window enforcement between user migrations (`CooldownNotElapsed`).
- Emergency pause controls and unpause recovery (`MigrationPaused`).
- Admin access control on parameter updates (`Unauthorized`).
