# Vote-escrow boost

The vote-escrow module (`stellar-lend/contracts/hello-world/src/vote_escrow.rs`, issue #1532) lets a governance-token holder lock tokens for a chosen duration. The locked amount carries a voting boost that is largest at the maximum lock and decays linearly to nothing at expiry.

This module owns the escrow ledger and the boost math. It is separate from the step-function boost on delegated votes in `governance::power`. Wiring this curve into proposal tallies, or exposing it through contract entrypoints, is a separate decision and is not part of this module yet.

## Lock rules

| Rule | Value |
| --- | --- |
| Shortest lock | `MIN_LOCK_SECONDS` = 7 days |
| Longest lock (create or extend) | `MAX_LOCK_SECONDS` = 4 years (365-day years) |
| Maximum boost | `MAX_BOOST_BPS` = 10,000 (+100%, so 2x at a full lock) |
| Amount | Must be strictly positive |
| One lock per owner | A second `create_lock` fails; use `increase_amount` |

## Boost curve

```text
remaining    = max(unlock_time - now, 0), capped at MAX_LOCK_SECONDS
boost_bps    = MAX_BOOST_BPS * remaining / MAX_LOCK_SECONDS
voting_power = amount + amount * boost_bps / 10_000
```

| Remaining lock | Boost | Weight of 1,000 locked |
| --- | --- | --- |
| 4 years (full) | +100% | 2,000 |
| 2 years | +50% | 1,500 |
| 1 week | about +0.48% | 1,004 |
| Expired, not yet withdrawn | 0% | 1,000 |

The weight falls back to 1x at expiry, so a stale lock never keeps an inflated boost.

## Operations

| Function | Who may call it | Effect |
| --- | --- | --- |
| `initialize(admin, token)` | Current protocol admin, once | Sets the escrowed governance token |
| `create_lock(owner, amount, duration)` | Owner | Transfers `amount` in and starts a lock ending `duration` from now |
| `increase_amount(owner, amount)` | Owner, active lock only | Adds tokens; the unlock time does not change |
| `extend_lock(owner, new_unlock_time)` | Owner, active lock only | Moves the unlock time later, at most 4 years from now |
| `withdraw(owner)` | Owner, after unlock | Returns the full amount and removes the lock |
| `voting_power(owner)` | Anyone (read) | Current weight from the boost curve |

## Errors

`VoteEscrowError` codes start at 300 so they do not collide with the oracle (1–11) or lending error enums.

| Code | Name | Meaning |
| --- | --- | --- |
| 300 | `Unauthorized` | Caller is not the protocol admin (`initialize`) |
| 301 | `AlreadyInitialized` | `initialize` has already run |
| 302 | `NotInitialized` | The escrow token has not been set |
| 303 | `InvalidAmount` | Amount is zero or negative |
| 304 | `InvalidLockDuration` | Duration outside the allowed range |
| 305 | `LockAlreadyExists` | Owner already has a lock |
| 306 | `LockNotFound` | Owner has no lock |
| 307 | `LockExpired` | Lock can no longer be increased or extended |
| 308 | `LockNotExpired` | Withdrawal attempted before unlock |
| 309 | `InvalidExtension` | New unlock time is not later than the current one |
| 310 | `Overflow` | An arithmetic step overflowed |

## Security notes

- Every state change requires the owner's authorization.
- `initialize` only accepts the current admin, and it runs once, so an outsider cannot pick the escrowed token.
- Tokens leave the escrow only through `withdraw`, and only after the unlock time.
- All arithmetic is checked. Amounts and durations are validated before any transfer.
