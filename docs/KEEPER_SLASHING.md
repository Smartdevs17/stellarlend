# Keeper submission slashing (Issue #1780)

## Behavior

A keeper submission is a signed claim about off-chain work. A misbehaving
submission (for example a conflicting attestation) can be slashed exactly once:
the keeper's staked collateral is debited, the submission is marked, aggregate
accounting is updated and a `ksl` event is published in one contract invocation.
Every check runs before any state write, so a rejected slash changes nothing.

Only two privileged callers may slash: the configured `slasher` (typically the
review service) or the `admin`. A random caller cannot slash even with a valid
submission id.

## Contract API

`contracts/keeper/src/slashing.rs` exposes `KeeperSlashingContract`:

| Function | Purpose |
| --- | --- |
| `initialize(admin, slasher, treasury, slash_bps)` | One-time setup; `slash_bps` must be in `1..=10000`. |
| `configure(admin, slasher, treasury, slash_bps)` | Admin-only rotation of privileged callers or rate. |
| `stake(keeper, amount)` | Keeper-authenticated collateral lock; amount must be positive. |
| `submit(keeper, payload_hash)` | Keeper-authenticated submission with a monotonically increasing id. |
| `slash(caller, submission_id, reason)` | Slasher/admin-only, one-shot; returns the slashed amount. |
| `get_config`, `get_submission`, `get_stake`, `get_stats` | Read-only views. |

Errors use typed `SlashingError` codes: unauthorized caller (1), missing
submission (5), already slashed (6), no stake (7), invalid amount/reason/bps.
The slash amount is `stake * slash_bps / 10_000`, with a minimum of 1 for a
positive stake.

## Backend review flow

`backend/src/services/keeperReview.ts` provides `KeeperReviewService`:

- `review(submission, decision, reviewer, reason?)` validates the submission,
  reviewer membership, decision and reason before calling the injected
  `SlashingClient`.
- A successful slash marks the submission reviewed and returns the outcome;
  `accept` records a review without slashing.
- Failures throw `KeeperReviewError` with a stable `code` and do not mark the
  submission, so the same id can be retried.
- Duplicate reviews are rejected with `ALREADY_REVIEWED`.

Observability is injected, not hard-wired: `logger.info/warn/error` receives
`keeper.review.accepted`, `keeper.review.slash_completed`,
`keeper.review.slash_failed` and `keeper.review.invalid_slash_outcome` events;
`metrics.increment` records `keeperReviewsTotal`, `keeperSlashTotal` and
`keeperSlashFailedTotal`. Each log includes submission id, keeper, reviewer and
amount/tx hash where relevant.

## Configuration and compatibility

The keeper crate is self-contained (`contracts/keeper/Cargo.toml`) so it can be
adopted without changing other workspace members. The backend service uses only
injected dependencies and relative imports; no existing service or route is
modified, so rollout is invisible until a caller opts in. Reviewers, slashing
client, logger, metrics and minimum reason length are all supplied by the caller.

## Validation

- Contract: `cargo test --manifest-path contracts/keeper/Cargo.toml` (5 tests).
- Backend: `node --experimental-strip-types --test backend/src/services/keeperReview.test.ts` (8 tests).
