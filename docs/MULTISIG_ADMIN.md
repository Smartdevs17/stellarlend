# Multi-Sig Protocol Admin (Issue #1390)

## Overview

StellarLend previously relied on a **single admin key** for all privileged
protocol operations (setting collateral ratios, interest rates, oracle address,
emergency pause, contract upgrades, etc.). A single compromised key was
sufficient to drain the protocol or manipulate its parameters.

This document describes the `multisig.rs` module that replaces that single-key
model with a **threshold multi-sig** (M-of-N) gate.

---

## Design

### M-of-N Threshold

Any number of signers `N` can be registered. A proposal becomes executable only
after `M` distinct signers approve it. Both `M` and `N` are configurable and
can themselves be changed through the multi-sig process.

```
N = total registered signers
M = required approvals (1 ≤ M ≤ N)
```

Recommended production configuration: **2-of-3** minimum.

### Execution Timelock

Every approved proposal is subject to a **48-hour timelock** before it can be
executed. This gives the community a window to detect and react to malicious or
accidental proposals before they take effect.

### Proposal lifecycle

```
OPEN → ms_propose(proposer, action)
         └── proposer auto-approves
PENDING → ms_approve(signer_2)
            └── if approval_count >= M → APPROVED
APPROVED → [wait 48 h]
READY → ms_execute(executor) → ProposalAction returned
EXECUTED (terminal)
```

Any admin may call `ms_cancel` on a PENDING or APPROVED proposal to
transition it to CANCELLED (terminal).

---

## Protected Operations

| ProposalAction | Effect |
|----------------|--------|
| `SetMinCollateralRatio(i128)` | Minimum collateral ratio in bps |
| `SetBaseBorrowRate(i128)` | Base interest rate in bps/year |
| `SetReserveFactor(i128)` | Protocol reserve factor in bps |
| `SetLiquidationIncentiveBps(i128)` | Liquidation bonus in bps |
| `SetOracle(Address)` | Oracle contract address |
| `PauseProtocol(bool)` | Emergency pause / unpause |
| `RotateAdmin(Address)` | Replace the privileged admin key |
| `UpgradeContract(BytesN<32>)` | Upgrade the lending WASM hash |

---

## API Reference

### `ms_set_admins(env, caller, admins, threshold)`

Configure (or bootstrap) the signer set.

- **Auth:** Any existing admin, or any caller if no admins are registered yet.
- `admins`: non-empty `Vec<Address>`; `threshold`: `1 ≤ M ≤ N`.

### `ms_propose(env, proposer, action) → Result<u64, MultisigError>`

Create a proposal. Proposer auto-approves. Returns the proposal ID.

### `ms_approve(env, approver, proposal_id) → Result<(), MultisigError>`

Add one approval. Transitions proposal to `Approved` when threshold is reached.

### `ms_cancel(env, canceller, proposal_id) → Result<(), MultisigError>`

Cancel an open proposal. Any admin may cancel.

### `ms_execute(env, executor, proposal_id) → Result<ProposalAction, MultisigError>`

Execute an approved, timelock-elapsed proposal. Returns the action to apply.

### View functions

| Function | Returns |
|----------|---------|
| `ms_get_admins(env)` | `Vec<Address>` |
| `ms_get_threshold(env)` | `u32` |
| `ms_get_proposal(env, id)` | `Option<MsProposal>` |
| `ms_get_approvals(env, id)` | `Vec<Address>` |

---

## Events

| Event symbol | Payload | Description |
|-------------|---------|-------------|
| `ms_admins` | `(threshold, count)` | Admin set updated |
| `ms_creatd` | `(id, proposer)` | Proposal created |
| `ms_apprvd` | `(id, approver)` | Proposal approved |
| `ms_execd`  | `(id, executor)` | Proposal executed |
| `ms_cancld` | `(id, canceller)` | Proposal cancelled |

---

## Security Properties

| Threat | Mitigation |
|--------|-----------|
| Single key compromise | M-of-N threshold — attacker needs M keys |
| Rushed execution | 48-hour timelock before execution |
| Proposal replay | `ProposalStatus::Executed` is terminal; no second execution |
| Front-running | Proposer auto-approves in the same call |
| Admin set takeover | Replacing admin set also requires M-of-N approval |

---

## Integration

1. Add `pub mod multisig;` to `stellar-lend/contracts/lending/src/lib.rs`.
2. Expose `ms_set_admins`, `ms_propose`, `ms_approve`, `ms_cancel`,
   `ms_execute`, and the view helpers on `LendingContract`.
3. In each admin entrypoint, replace the direct parameter mutation with a call
   through `ms_execute` / `ms_propose`.

---

## File locations

| Path | Description |
|------|-------------|
| `stellar-lend/contracts/lending/src/multisig.rs` | Implementation |
| `docs/MULTISIG_ADMIN.md` | This document |
