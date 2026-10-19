# Governance Lifecycle

StellarLend’s on-chain governance (`hello-world` contract, `gov_*` entrypoints) implements a full proposal lifecycle with timelock, quorum, and voting-threshold gates. This document describes the state machine, configuration knobs, and how the lifecycle is exercised in E2E tests and gas benchmarks (Issue #690).

## Proposal State Machine

```
 create ──► Active ──(end_time)──► Succeeded ──queue──► Queued ──(eta)──► Executed
              │                        │                  │
              │                        └─(grace)─► Expired└─(eta + grace)─► Expired
              └──(end_time, fails)──► Defeated
 Pending / Active / Queued ──cancel (proposer, admin, guardian)──► Cancelled
```

Only transitions made by a successful call are written to storage. States that
follow from time alone (voting closed, grace period lapsed) are derived on read
by `gov_get_proposal_state`, because a call that fails cannot persist anything.
Clients should read state from `gov_get_proposal_state`, not from the stored
`Proposal.status`.

| Status | Meaning |
|--------|---------|
| `Pending` | Stored at creation; reported as `Active` once `now >= start_time` |
| `Active` | Voting open: `start_time <= now < end_time` |
| `Succeeded` | Voting closed, quorum and threshold met, not yet queued (derived) |
| `Defeated` | Voting closed without quorum or threshold (derived, stored by `queue`) |
| `Queued` | In the timelock; executable from `execution_time` (eta) |
| `Executed` | Applied by `execute_proposal` (terminal) |
| `Expired` | Not queued within the grace period after `end_time`, or not executed within the grace period after eta (derived, terminal) |
| `Cancelled` | Cancelled before execution (terminal) |

## Voting Power (lock-to-vote)

Soroban tokens expose neither historical balances nor a total supply, so
governance cannot snapshot holders by reading the vote token. Instead:

- Holders lock vote tokens with `gov_lock_tokens` and withdraw them with
  `gov_unlock_tokens`. Locked tokens are held by the contract.
- Every change to an account's voting power, or to the total locked supply,
  writes a timestamped checkpoint.
- A proposal counts power held **strictly before** the timestamp it was created
  at. Tokens borrowed, bought, locked or delegated in the proposal's own ledger
  or later carry no weight on it. This is what stops flash-loan voting and
  vote-then-transfer double voting.
- After voting, a voter's locked tokens and delegation stay fixed until the
  voting period of every proposal they voted on has ended (`gov_is_vote_locked`).

### Delegation

`gov_delegate_vote(delegator, delegatee)` moves the voting power of the
delegator's locked tokens, including tokens locked later, to the delegatee.
`gov_revoke_delegation` returns it. Calling `gov_delegate_vote` again
re-delegates.

Delegation is single-hop. Power received by delegation cannot be delegated
onward. A delegatee can still delegate its *own* locked tokens, so chains and
cycles cannot form and no depth limit is needed. Delegation counts on a proposal
only if it predates the proposal's creation, like any other change to voting
power.

## Configuration (`gov_initialize`)

```rust
gov_initialize(
    admin: Address,
    vote_token: Address,
    voting_period: Option<u64>,        // seconds; 1 hour ..= 30 days
    execution_delay: Option<u64>,      // seconds; timelock between queue and eta, <= 30 days
    quorum_bps: Option<u32>,           // share of locked supply that must vote; 1 ..= 10_000
    proposal_threshold: Option<i128>,  // min voting power to create a proposal
    timelock_duration: Option<u64>,    // seconds; grace period to queue / execute, 1 ..= 30 days
    default_voting_threshold: Option<i128>, // bps of votes cast that must be `For`; 1 ..= 10_000
)
```

Defaults fall back to contract-level constants (`DEFAULT_TIMELOCK_DURATION`,
etc.) when `None`. Out-of-bounds values are rejected.

After initialization, the rules change only through governance itself: an
`UpdateGovernanceConfig(GovernanceParams)` proposal passes a vote and the
timelock like any other. Its bounds are re-checked at execution. A proposal keeps
the voting period, quorum and threshold it was created under.

## Key Invariants Enforced On-Chain

1. **Proposal threshold**: the proposer's voting power before the current ledger is at least `proposal_threshold`.
2. **Vote power**: checkpointed power strictly before `created_at`. Voters with zero power are rejected (`NoVotingPower`).
3. **One vote per address**: a second vote reverts with `AlreadyVoted`.
4. **Voting window**: votes are accepted only while `start_time <= now < end_time` (`NotInVotingPeriod`).
5. **Quorum**: `for + against + abstain >= quorum_votes`, where `quorum_votes = ceil(total_locked_before_creation * quorum_bps / 10_000)` is fixed at creation.
6. **Threshold**: `for >= (for + against + abstain) * voting_threshold / 10_000` and `for > against`, so a tie never passes. A proposer may raise `voting_threshold` above the default, never lower it.
7. **Execution delay**: `execute` rejects while `now < eta` (`ExecutionTooEarly`), where `eta = queue time + execution_delay`.
8. **Grace period**: a proposal must be queued within `timelock_duration` of `end_time`, and executed within `timelock_duration` of eta, or it expires.
9. **Veto window**: the proposer, admin, or any guardian can cancel a proposal until it executes, including while it is queued.
10. **Emergency proposals** skip voting and the delay. Each needs approvals (`gov_approve_proposal`) from the multisig threshold of current multisig admins, and a multisig admin must execute it.

## Entrypoints (all under `#[contractimpl]`)

| Entrypoint | Purpose |
|-----------|---------|
| `gov_initialize` | One-time config + vote token setup |
| `gov_lock_tokens` / `gov_unlock_tokens` | Lock vote tokens for voting power / withdraw them |
| `gov_delegate_vote` / `gov_revoke_delegation` | Delegate locked-token power / take it back |
| `gov_get_votes` / `gov_get_past_votes` | Current voting power / power strictly before a timestamp |
| `gov_get_locked_balance` / `gov_get_total_locked` / `gov_get_delegation` | Lock and delegation views |
| `gov_create_proposal` | Open a proposal (threshold-gated) |
| `gov_vote` | Cast `For`/`Against`/`Abstain` with checkpointed power |
| `gov_get_vote_power_snapshot` | The power a voter can use on a proposal |
| `gov_queue_proposal` | Tally after voting ends → `Queued` or `Defeated` |
| `gov_execute_proposal` | Apply a queued proposal after the delay, within the grace period |
| `gov_cancel_proposal` | Cancel / veto by proposer, admin or guardian |
| `gov_get_proposal` / `gov_get_proposal_state` | Stored proposal / current lifecycle state |
| `gov_get_config` | Read current config |
| `gov_create_emergency_proposal` / `gov_approve_proposal` | Multisig emergency path |
| `gov_add_guardian` | Register a guardian (can veto proposals) |

## E2E Coverage

`tests/e2e/governance.e2e.test.ts` exercises the full lifecycle against the in-memory harness (`buildGovernanceApp` in `tests/e2e/scenarios/harness.ts`):

- Initialization and re-init rejection
- Proposal creation + threshold enforcement
- Vote activation, double-vote rejection, zero-power rejection
- Queue gating (before voting ends), success → `Queued`, failure → `Defeated`
- Execute gating (before delay → 409; after delay → `Executed`; past timelock → 410/`Expired`)
- Cancel by proposer; cancel of executed proposal rejected
- End-to-end happy path with event trace

Run:

```bash
cd tests/e2e
npm ci
npx jest --runInBand --forceExit governance.e2e.test.ts
```

## Gas Benchmarks

`stellar-lend/benchmarks/src/governance_benchmarks.rs` (group: **Governance Lifecycle**) measures instruction counts for:

- `gov_initialize`
- `gov_create_proposal`
- `gov_vote`
- `gov_queue_proposal`
- `gov_execute_proposal`
- `gov_cancel_proposal`
- `gov_get_proposal`
- `gov_get_governance_config`

Budgets are registered in `framework.rs` (`hello_world::gov_*` keys). Run:

```bash
cd stellar-lend
cargo run --bin run_benchmarks -- --output governance-bench.json
```

## CI

`.github/workflows/governance-testing.yml` runs the governance E2E suite on PRs touching governance paths and nightly.

## Related Specs

- Rust lifecycle tests: `stellar-lend/contracts/hello-world/src/tests/governance_{test,lifecycle_test,attack_prevention_test}.rs`
- State machine source: `stellar-lend/contracts/hello-world/src/governance/{proposal,voting,power,execution}.rs`
