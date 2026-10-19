# Supplier rewards

The modular `stellar-lend/contracts/lending` contract supports an opt-in reward
pool funded with a SEP-41 token. The protocol admin allocates cumulative rewards
to suppliers; each supplier authorizes a claim directly to their own address.
This is an allocation-based incentive program, not automatic interest or
time-based emissions.

## Eligibility and trust

The current modular deposit API records balances without transferring deposit
tokens. A deposit balance is therefore **not proof of funded supply**. The admin
must independently verify the supplier's eligibility and backing before setting
an entitlement. A positive deposit balance is a necessary on-chain check, not a
sufficient one. Neither deposits nor elapsed time automatically earn rewards.
The admin controls allocation policy; callers cannot allocate their own rewards.

This feature does not change deposit custody, borrowing, or withdrawal behavior.
Do not use deposit balance alone to automate allocations until custody is enforced.

## Configuration and funding

1. Initialize the lending contract normally.
2. The stored protocol admin calls
   `configure_supplier_rewards(admin, reward_token, true)`.
3. A funder authorizes `fund_supplier_rewards(funder, amount)`. Tokens actually
   move from the funder to the lending contract; the budget increases only if
   both sender and recipient balances change by exactly `amount`.
4. The admin calls `set_supplier_reward(admin, supplier, lifetime_total)` after
   verifying eligibility. Only the increase over the previous lifetime total
   consumes the available budget. It cannot exceed that budget or lower an
   existing entitlement. The supplier must currently have a positive deposit
   balance for a new allocation.

Amounts are raw reward-token units. The reward token is immutable once configured,
even while the pool is empty, so outstanding entitlements never change currency.
Only trusted exact-transfer SEP-41 tokens are supported; fee-on-transfer behavior
is rejected. Token failures revert the entire operation and its accounting.
Direct transfers to the contract do not create an allocation budget; use the
funding entrypoint. Unallocated funding has no admin withdrawal entrypoint.

Calling `configure_supplier_rewards(admin, same_token, false)` pauses **new
allocations**, not funding or claims. It never erases earned rewards. Re-enable
with the same token and `true`. There is no automatic rate, expiry, or clawback.

## Claims and retries

`claim_supplier_rewards(supplier)` requires that supplier's authorization and
transfers `allocated - claimed` to that same address. There is no arbitrary payout
recipient. Claims remain available after a supplier withdraws or new allocations
are disabled. Accounting is updated before the external token transfer, and the
shared lending reentrancy guard blocks token callbacks into reward mutations.

For example, allocating a lifetime total of 300 and claiming pays 300. Repeating
either call does not pay again. Raising the lifetime total to 450 makes only 150
more claimable. A claim with nothing pending returns zero and emits no claim event.

## Queries and events

- `get_supplier_rewards_config()` returns the optional token/enabled configuration.
- `get_supplier_reward(user)` returns lifetime `allocated` and `claimed` amounts.
  Their difference is the claimable amount.
- `get_supplier_reward_pool()` returns `available`, `reserved`, and `paid` counters.
  Successful funding increases `available`; allocations move budget to `reserved`;
  claims move it to `paid`. New allocations never spend tokens owed to another user.
- Typed events: `SupplierRewardsConfigured`, `SupplierRewardsFunded`,
  `SupplierRewardAllocated`, and `SupplierRewardsClaimed`.
- Typed `SupplierRewardsError` values are registered under lending error domain 26.

## Verification

From `stellar-lend/`:

```sh
cargo test -p stellarlend-lending --test supplier_rewards
cargo test -p stellarlend-errors
```

The supplier suite uses the shared integration fixture and real Stellar Asset
Contracts. It checks funding, allocation caps, multiple suppliers, repeat calls,
authorization, disabled allocations, withdrawal before claiming, and rollback.
An adversarial token tests callback blocking and exact-transfer enforcement.
The suite is included in `scripts/testing/run-contract-tests.sh`'s
`lending-integration` group, so the contract CI and coverage jobs run it.
