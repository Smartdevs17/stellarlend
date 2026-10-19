# Reusable Reentrancy Guard (`stellarlend-security`)

The `contracts/security` crate provides a **single, reusable** reentrancy
guard shared by all StellarLend contracts. Before this crate, each contract
(hello-world, lending) maintained its own `reentrancy.rs` copy with slightly
different error types and storage keys.

This crate centralises the primitive while preserving each contract's public
behaviour: contracts wrap the shared guard and map [`ReentrancyError`] onto
their own error enums at the call site.

## Features

1. **Function-level guards** — per-function locks via [`ReentrancyKey`]
   (`GlobalLock`, `DepositLock`, `WithdrawLock`, `BorrowLock`, `RepayLock`,
   `LiquidateLock`, `FlashLoanLock`, `DepositCollateralLock`, or a
   caller-supplied `Custom(u32)`).
2. **Cross-contract guards** — a caller-bound lock (`new_with_caller`) so a
   re-entering contract is detected even when the underlying function key
   differs.
3. **Read-only guards** — read-only functions may re-enter, but the
   re-entrancy is tracked and surfaced through
   [`ReentrancyGuard::is_read_only_reentrancy`].
4. **Constructor guards** — block re-initialisation of a contract.
5. **Delegate-call guards** — block delegate-call re-entrancy.
6. **Guard configuration** — [`ReentrancyGuardConfig`] and
   [`ReentrancyGuard::configure`] allow contracts to wire and summarise a
   bundle of guards in one place.
7. **Testing utilities** — the `testutils` feature exposes `testing` helpers
   (e.g. `assert_function_guard_blocks_reentry`,
   `assert_cross_contract_guard`) for consuming contracts.

## Semantics

The guard follows the checks-effects-interactions (CEI) pattern. A
temporary-storage marker is written on construction and removed on `Drop`
(even on panic), so locks can never be left dangling.

| Constructor | Key | Re-entry behaviour |
|---|---|---|
| `new` | `GlobalLock` | Rejected |
| `new_with_key` | chosen key | Rejected for write guards; tracked for read-only |
| `new_with_caller` | chosen key + caller | Rejected (cross-contract) |
| `new_constructor` | `ConstructorLock` | Rejected |
| `new_delegate_call` | `DelegateCallLock` | Rejected |
| `new_read_only` | `ReadOnlyLock` | Allowed but tracked |

## Errors

[`ReentrancyError`] is a `#[contracterror]` enum:

| Code | Variant |
|---|---|
| 1 | `ReentrancyDetected` |
| 2 | `CrossContractReentrancy` |
| 3 | `ConstructorReentrancy` |
| 4 | `DelegateCallReentrancy` |

Consuming contracts map these onto their own domain error enums, e.g.
`ReentrancyGuard::new(env).map_err(|_| DepositError::Reentrancy)?`.

## Usage

```rust,ignore
use stellarlend_security::{ReentrancyGuard, ReentrancyKey};

fn deposit(env: &Env, ...) -> Result<(), DepositError> {
    let _guard = ReentrancyGuard::new(env).map_err(|_| DepositError::Reentrancy)?;
    // ... effects ...
    Ok(())
}
```

## Consuming contract migration

- `hello-world/src/reentrancy.rs` and `lending/src/reentrancy.rs` are now thin
  wrappers. All locking logic lives in this crate; the wrappers only map
  [`ReentrancyKey`] onto the shared key space and map [`ReentrancyError`] onto
  the contract-local error type (bare `u32` for hello-world, `ReentrancyError`
  for lending).

## Benchmarks

The crate's locking primitives are exercised through the contract facing
benchmarks under `stellar-lend/benchmarks/`. A dedicated guard benchmark can
be added using the standard framework (`Register a benchmark that acquires and
releases several guard keys and asserts the instruction/cost budget`).
