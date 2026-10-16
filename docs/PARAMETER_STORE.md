# On-Chain Parameter Store

Reference for the governance-controlled pool parameter store (issue #697).

Pool configuration used to live off chain or hardcoded in the contracts.
That made every parameter change a deployment, put the current value of a risk
parameter beyond the reach of anyone who could not read the source, and left no
record of who changed what or when. The parameter store moves pool configuration
on chain, behind governance, with versioning, validation, timelocks, and impact
simulation.

The design premise: a parameter change on a lending protocol is a risk event.
LTV and liquidation threshold decide who gets liquidated. So the store is built
to make a change slow, visible, reversible, and simulatable before it lands —
not merely to store numbers on chain.

## Contract

`stellar-lend/contracts/parameter-store/`

| Module | Contents |
| --- | --- |
| `lib.rs` | `ParameterType`, proposal lifecycle, versioning, storage |
| `voting.rs` | `VotingConfig`, vote tallying, quorum and approval rules |
| `simulation.rs` | Impact simulation against a pool snapshot |
| `hello_world_bridge.rs` | Reads parameters from the main lending contract |

### Parameters

| Parameter | Valid range (bps unless noted) | Timelock |
| --- | --- | --- |
| `LTV` | 1 – 9000 | 48h |
| `LiquidationThreshold` | 1 – 10000 | 48h |
| `CloseFactor` | 1 – 10000 | 48h |
| `LiquidationIncentive` | 1000 – 2000 | 48h |
| `ReserveFactor` | 0 – 10000 | 24h |
| `DebtCeiling` | >= 0 (absolute) | 24h |
| `BaseInterestRate` | see `validate_range` | 24h |
| `Slope1`, `Slope2` | see `validate_range` | 24h |
| `OptimalUtilization` | see `validate_range` | 24h |

The four risk parameters carry a 48-hour timelock rather than 24. They are the
ones that can liquidate users, so they get twice the window for someone to
notice and object. Emergency changes use a 4-hour timelock, which is short
enough to respond to an incident and long enough to still be observed.

Ranges are enforced by `validate_range` at proposal time, not at execution time.
A proposal that could never be applied should never enter the queue and consume
a governance cycle.

### Lifecycle

```
propose_change ──▶ [pending] ──▶ cast_vote ──▶ tally
                       │                         │
                       │                    has_passed?
                       │                         │
                       ├── reject_proposal ──────┴──▶ [rejected]
                       │
                       └── accept_proposal (after timelock) ──▶ [accepted]
                                                                    │
                                                                    ▼
                                                          version += 1, history entry
```

`propose_emergency_change` shortens the timelock and sets an emergency flag,
readable via `is_emergency_active`. `execute_emergency_override` applies it and
`clear_emergency_override` resets the flag. The emergency path is deliberately
separate and separately observable, so it cannot be used quietly as a faster
ordinary path.

### Voting

`VotingConfig` sets the voting period, quorum, and approval threshold. Voting
power is assigned per address via `set_voting_power` and tallied by
`tally_votes`:

- `meets_quorum` — participation against total voting power.
- `is_approved` — support against the approval threshold.
- `has_passed` — both together.

Quorum and approval are checked separately because they fail for different
reasons. A proposal that nobody voted on is not the same as one the electorate
rejected, and collapsing them into a single boolean loses the distinction that
tells governance whether to re-run the vote or drop the proposal.

`has_voted` prevents double voting; `is_voting_open` bounds the window.

### Versioning

Every accepted change increments the parameter's version and appends a history
entry. `get_parameter_version` returns the current version and
`get_parameter_at_version` reads any historical value.

Versioned history is what makes a change reversible in practice: rolling back
means reading the previous version and proposing it, rather than reconstructing
what the value used to be from memory or from a chain explorer.

### Impact simulation

`simulate_change` and `simulate_proposal` evaluate a proposed value against a
`PoolSnapshot` and return a `ParameterImpact` carrying an `ImpactSeverity`
alongside the projected effects, plus `RelatedParameters` for the values that
interact with the one being changed.

The related-parameters output exists because these parameters are not
independent. Raising LTV without checking the liquidation threshold narrows the
band between "can borrow" and "gets liquidated", and a simulation that reported
only the changed parameter would make that look safe.

Simulation is advisory — it does not gate acceptance. Governance decides; the
simulation makes sure it decides with the consequences in view.

## API

All routes under `/api/parameters`.

### Reading

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/pools` | Registered pools |
| `GET` | `/:pool` | All parameters for a pool |
| `GET` | `/:pool/:parameter` | One parameter's current value |
| `GET` | `/:pool/:parameter/history` | Full change history |
| `GET` | `/:pool/:parameter/versions/:version` | Value at a specific version |

### Proposing and voting

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/pools` | Register a pool |
| `POST` | `/proposals` | Propose a change |
| `POST` | `/proposals/emergency` | Propose an emergency change |
| `GET` | `/proposals` | List proposals (`?pool=`, `?status=`) |
| `GET` | `/proposals/:id` | One proposal |
| `POST` | `/proposals/:id/accept` | Accept after timelock |
| `POST` | `/proposals/:id/reject` | Reject |
| `POST` | `/proposals/:id/votes` | Cast a vote |
| `GET` | `/proposals/:id/votes` | Votes and tally |

`frontend/src/components/GovernanceVoting.tsx` is the voting interface for
these endpoints. It lists pending proposals and their voting deadline, and
shows the tally against the quorum and approval threshold. The connected
wallet (`voterAddress`) can vote for or against while the window is open.

### Simulation, validation, configuration

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/simulate` | Simulate a change against a pool snapshot |
| `POST` | `/validate` | Validate a value without proposing |
| `GET` | `/voting/config` | Current voting configuration |
| `PUT` | `/voting/config` | Update voting configuration |
| `PUT` | `/voting/power` | Set an address's voting power |
| `GET` | `/notifications` | Recent parameter change notifications (`?limit=`, `?pool=`) |

`POST /api/parameters/validate` lets a UI check a value before a user commits to
a proposal, using the same `validate_range` rules the contract enforces — so the
form and the chain cannot disagree about what is acceptable.

### Change notifications

`GET /api/parameters/notifications` returns recent accepted changes, newest
first. Integrators poll this to learn that a parameter moved rather than
discovering it from changed protocol behaviour.

## Example

Proposing an LTV reduction:

```bash
# 1. Check the value is acceptable before spending a governance cycle on it.
curl -X POST /api/parameters/validate \
  -H 'Content-Type: application/json' \
  -d '{"pool": "POOL_ADDR", "parameter": "LTV", "value": 7000}'

# 2. Simulate its effect on the current pool state.
curl -X POST /api/parameters/simulate \
  -H 'Content-Type: application/json' \
  -d '{"pool": "POOL_ADDR", "parameter": "LTV", "value": 7000, "snapshot": { }}'

# 3. Propose it. LTV is a risk parameter, so the timelock is 48h.
curl -X POST /api/parameters/proposals \
  -H 'Content-Type: application/json' \
  -d '{"pool": "POOL_ADDR", "parameter": "LTV", "value": 7000, "proposer": "G..."}'

# 4. Vote, then accept once the timelock has elapsed and the vote has passed.
curl -X POST /api/parameters/proposals/1/votes \
  -H 'Content-Type: application/json' \
  -d '{"voter": "G...", "support": true}'

curl -X POST /api/parameters/proposals/1/accept
```

## Testing

`cargo test -p parameter-store` — 42 tests covering the proposal lifecycle,
timelock enforcement, range validation, voting quorum and approval, versioning,
and simulation.

## Related

- [`GOVERNANCE.md`](./GOVERNANCE.md) — the wider governance system.
- [`MATH_LIBRARY.md`](./MATH_LIBRARY.md) — arithmetic used by simulation.
- [`PROTOCOL_HEALTH_SCORE.md`](./PROTOCOL_HEALTH_SCORE.md) — governance health
  feeds the protocol health score.
