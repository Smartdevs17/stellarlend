# Lending Hot-Path Optimisations

This document covers four related changes to the `stellarlend-lending` contract.
They reduce ledger-entry reads and writes, per-pool rent, and transaction size
on the most frequently used code paths.

| Issue | Change | Module |
|-------|--------|--------|
| #1043 | Storage packing for hot paths | `src/hot_storage.rs` |
| #1044 | Batch deposits | `src/deposit_batch.rs` |
| #1045 | Calldata compression | `src/calldata.rs` |
| #1046 | Lazy pool-state initialisation | `src/lazy.rs`, `src/hot_storage.rs`, `src/borrow.rs` |

## 1. Storage packing (#1043)

### Deposit state

`deposit` used to read `TotalAmount`, `CapAmount` and `MinAmount` as three
separate persistent entries. `withdraw`, `emergency_withdraw` and
`sweep_deposit_dust` each read and wrote `TotalAmount`. These three values now
live in a single `HotStorageKey::DepositState` entry (`DepositHotState`):

| Path | Before | After |
|------|--------|-------|
| `deposit` | 3 reads + 1 write | 1 read + 1 write |
| `withdraw` / `emergency_withdraw` / `sweep_deposit_dust` | 1 read + 1 write | 1 read + 1 write (on the shared packed entry) |
| `initialize_deposit_settings` | 2 writes | 1 read + 1 write |

### Borrow limits

`BorrowDebtCeiling` and `BorrowMinAmount` are read on every borrow. They are now
packed into `HotStorageKey::BorrowLimits`, so a borrow does one configuration
read instead of two.

### Migration

Both slots read through to the legacy keys when the packed entry is missing:

* **Deposit state:** the first hot-path write after an upgrade stores the packed
  entry and removes the legacy keys. This needs no extra transaction.
* **Borrow limits:** the migration happens on the next
  `initialize_borrow_settings`.
* **Either one, eagerly:** an admin can call `migrate_pool_state`.

## 2. Batch deposits (#1044)

```rust
deposit_batch(user: Address, requests: Vec<DepositRequest>) -> BatchDepositResult
```

* Accepts up to `MAX_BATCH_DEPOSITS` (20) entries.
* Performs one `require_auth`, one reentrancy guard and one pause check.
* Reads and writes the packed deposit state once, and the user position once.
* **All-or-nothing:** every entry is validated before anything is written, and
  the deposit cap is checked against the cumulative batch total.
* Emits one `VaultDepositEvent` per entry, with the running balance, plus a
  `BatchDepositEvent` summary. Indexers see the same per-deposit events that N
  sequential `deposit` calls would produce.
* New errors: `DepositError::EmptyBatch` (8) and `DepositError::BatchTooLarge` (9).

## 3. Calldata compression (#1045)

`execute_compressed(user, payload: Bytes)` runs a sequence of operations
encoded in a compact binary format. The payload is atomic and authorises the
user once.

```text
payload := version:u8(=1)  count:u8(1..=32)  op{count}
op      := header:u8  [ext_index:u8]  amount:leb128
header  := opcode << 4 | short_index      (short_index 0xF => ext_index follows)
```

| Opcode | Operation |
|--------|-----------|
| 1 | `deposit` |
| 2 | `withdraw` |
| 3 | `repay` |
| 4 | `deposit_collateral` |

* **Assets** are 1-byte indices into an admin-managed dictionary. The admin sets
  it with `set_calldata_assets` and clients read it with `get_calldata_assets`.
  A payload therefore carries 1–2 bytes per asset instead of a 32-byte address.
* **Amounts** are canonical unsigned LEB128 values in `1..=i128::MAX`. Typical
  token amounts take 3–9 bytes instead of 16.
* **Rejected payloads:** trailing bytes, overlong or non-canonical varints,
  unknown opcodes and unknown asset indices all fail decoding.
* **Deposit coalescing:** consecutive `Deposit` ops are merged into one
  `deposit_batch` call, which also gives them the batch storage savings.
* **Client helpers:** `encode_calldata` and `decode_calldata` are read-only
  entrypoints, so clients can build and inspect payloads through simulation.

## 4. Lazy pool-state initialisation (#1046)

A new pool no longer pays rent for state it has not used yet:

* The existing `lazy` module is now compiled into the contract.
  * `AccumulatedFees` is created by the first fee-bearing flash loan or
    emergency withdrawal.
  * `BorrowIndexSnapshot` is created by the pool's first borrow.
* The packed deposit state is created by the first deposit, withdraw or settings
  update. Until then, reads return `DepositHotState::DEFAULT` without allocating
  storage.
* `initialize_borrow_settings` no longer writes these defaults eagerly, because
  their getters already fall back to the `DEFAULT_*` constants:
  * the stable-rate premium,
  * the stable-rate recalculation interval,
  * the rate-switch fee.

### New entrypoints

* `get_pool_state()` returns a `PoolStateView` of every lazy field.
  `initialized_mask` shows which fields have been written.
* `get_pool_state_field(field)` returns the value of a single field.
* `migrate_pool_state(admin)` is idempotent. It writes every lazy field and
  folds legacy hot-path keys into their packed slots.
