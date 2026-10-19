# Protocol Treasury Controls

Summary of the controls that govern the StellarLend treasury, the audit trail
each one leaves, and the tests that cover them. See also `TREASURY_FEATURES.md`
(API layer) and `docs/reserve.md`.

## On-chain controls (`stellar-lend/contracts/hello-world/src/treasury.rs`)

| Control | Function | Guard | Audit event |
|---|---|---|---|
| Set treasury address | `set_treasury` | admin + `require_auth` | `treasury_set` |
| Set fee config (interest / liquidation bps) | `set_fee_config` | admin; each fee in 0..=10000 bps | `fee_config_updated` |
| Claim reserves | `claim_reserves` | admin; amount > 0; amount <= reserve; reentrancy guard | `reserves_claimed` |
| Accrue reserves | `accrue_reserve` | internal; checked arithmetic | reserve balance |

Multi-party custody is available through the `treasury-multisig` contract.

## Off-chain controls (`api/src/services/treasury-*.service.ts`)

- Rebalancer: target allocations must sum to 100%, pause/resume, governance-approved changes.
- Yield harvester: protocol whitelist, per-protocol max allocation, emergency withdrawal log.
- Protocol-owned liquidity: governance proposal + timelock for deploy/withdraw/rebalance.

## Audit trail

Every state-changing treasury action emits a structured event
(`docs/STRUCTURED_EVENT_SCHEMA.md`) carrying the acting admin, amounts and the
ledger timestamp, so the indexer can reconstruct the full history.

## Tests

- Contract: `stellar-lend/contracts/hello-world/src/treasury_test.rs`,
  `src/tests/reserve_test.rs`, `src/tests/access_control_regression_test.rs`
- API / TS: `stellar-lend/contracts/hello-world/tests/treasury.test.ts`
