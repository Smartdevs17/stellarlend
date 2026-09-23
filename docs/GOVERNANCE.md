# Governance Lifecycle

StellarLend’s on-chain governance (`hello-world` contract, `gov_*` entrypoints) implements a full proposal lifecycle with timelock, quorum, and voting-threshold gates. This document describes the state machine, configuration knobs, and how the lifecycle is exercised in E2E tests and gas benchmarks (Issue #690).

## Proposal State Machine

```
                 create_proposal
                       │
                       ▼
                   ┌────────┐   first vote    ┌────────┐
                   │ Pending│ ───────────────▶│ Active │
                   └────────┘                 └────────┘
                       │                          │
                       │ cancel (proposer/admin)  │ queue_proposal (after voting ends)
                       ▼                          ▼
                  ┌───────────┐   success   ┌─────────┐
                  │ Cancelled │             │ Queued  │
                  └───────────┘             └─────────┘
                                                │
                          execute (delay elapsed)│ execute (delay elapsed)
                           ┌────────────────────┼────────────────────┐
                           ▼                    ▼                    ▼
                      ┌──────────┐         ┌──────────┐        ┌──────────┐
                      │ Executed │         │ Defeated │        │ Expired  │
                      └──────────┘         └──────────┘        └──────────┘
```

Statuses (mirrors `types::ProposalStatus`):

| Status | Entered by | Terminal? |
|--------|-----------|-----------|
| `Pending` | `create_proposal` | No |
| `Active` | first `vote` when `now >= start_time` | No |
| `Queued` | `queue_proposal` when quorum + threshold met | No |
| `Executed` | `execute_proposal` after execution delay, within timelock window | Yes |
| `Defeated` | `queue_proposal` when quorum or threshold fails | Yes |
| `Expired` | `queue_proposal` past voting+timelock, or `execute_proposal` past timelock window | Yes |
| `Cancelled` | `cancel_proposal` by proposer or admin (only from non-queued/executed states) | Yes |
| `Succeeded` | intermediate outcome flag (set internally on successful vote tally) | No |

## Configuration (`gov_initialize`)

```rust
gov_initialize(
    admin: Address,
    vote_token: Address,
    voting_period: Option<u64>,        // seconds; proposal active window
    execution_delay: Option<u64>,      // seconds; queued → executable delay
    quorum_bps: Option<u32>,           // basis points of total votes required
    proposal_threshold: Option<i128>,  // min token balance to create proposal
    timelock_duration: Option<u64>,    // seconds; executable window after delay
    default_voting_threshold: Option<i128>, // bps of for-votes required
)
```

Defaults fall back to contract-level constants (`DEFAULT_TIMELOCK_DURATION`, etc.) when `None`.

## Key Invariants Enforced On-Chain

1. **Proposal threshold**: `balance(proposer) >= proposal_threshold` (unless threshold is 0).
2. **Vote power**: snapshot-based (`VotePowerSnapshot`) with optional delegation; zero-power voters are rejected (`NoVotingPower`).
3. **One vote per address**: double-vote reverts with `AlreadyVoted`.
4. **Quorum**: `total_votes >= (total_votes * quorum_bps) / 10_000` (evaluated at queue time).
5. **Voting threshold**: `for_votes >= (total_voting_power * voting_threshold) / 10_000`.
6. **Execution delay**: `execute` rejects if `now < execution_time` (`ExecutionTooEarly`).
7. **Timelock window**: `execute` expires the proposal if `now > execution_time + timelock_duration`.
8. **Cancel authorization**: only `proposer` or `admin`; cannot cancel `Queued` or `Executed`.

## Entrypoints (all under `#[contractimpl]`)

| Entrypoint | Purpose |
|-----------|---------|
| `gov_initialize` | One-time config + vote token setup |
| `gov_create_proposal` | Open a proposal (threshold-gated) |
| `gov_vote` | Cast `For`/`Against`/`Abstain` with snapshotted power |
| `gov_queue_proposal` | Tally after voting ends → `Queued` or `Defeated` |
| `gov_execute_proposal` | Apply queued proposal after delay, within timelock |
| `gov_cancel_proposal` | Cancel by proposer/admin |
| `gov_get_proposal` | Read proposal state |
| `gov_get_governance_config` | Read current config |
| `gov_add_guardian` | Register a social-recovery guardian |
| `gov_approve_proposal` | Multisig/guardian approval path |

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

- Rust lifecycle tests: `stellar-lend/contracts/hello-world/src/tests/governance_test.rs`
- State machine source: `stellar-lend/contracts/hello-world/src/governance/{proposal,voting,execution}.rs`
