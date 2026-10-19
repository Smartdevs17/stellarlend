# Contract Integration Guide

How to build on StellarLend: connect to the lending contract, walk common flows,
handle errors, and follow the integrator security checklist. The code here mirrors
the type-checked examples in [`packages/sdk/examples/`](../packages/sdk/examples/README.md);
copy them into your project or import them directly.

## Install and connect

```bash
npm install @stellarlend/sdk @stellar/stellar-sdk
```

`StellarLendClient` is a thin façade over the lending entry points; transaction
building, simulation, and signing stay in a `ContractAdapter` you provide. The
SDK ships three adapter examples:

| Adapter | Use for |
| --- | --- |
| [`SorobanRpcAdapter`](../packages/sdk/examples/rpc-adapter.ts) | Servers, bots, scripts (builds, simulates, signs, submits, polls) |
| [`WalletManager`](../packages/sdk/examples/wallet.ts) | Browser wallets (Freighter-style), user-signed transactions |
| [`FakeAdapter`](../packages/sdk/examples/support/fake-adapter.ts) | Unit tests — no network |

```ts
import { StellarLendClient } from '@stellarlend/sdk';
import { SorobanRpcAdapter, keypairSigner } from '@stellarlend/sdk/examples/rpc-adapter';

const adapter = new SorobanRpcAdapter({
  rpcUrl: 'https://soroban-testnet.stellar.org',           // or http://localhost:8000/rpc for the devnet
  networkPassphrase: 'Test SDF Network ; September 2015',  // standalone passphrase differs; see the devnet docs
  source: ADMIN_ADDRESS,
  signTransaction: keypairSigner(ADMIN_SECRET),            // server-side signer; never ship secrets to browsers
});
const client = new StellarLendClient({ contractId: LENDING_CONTRACT_ID, adapter });
```

Where to get contract IDs:

- **Local devnet**: `.devnet/state.json`, written by `npm run devnet` (see
  [`docker/devnet/README.md`](../docker/devnet/README.md)).
- **Testnet**: deploy and initialize with `stellar contract deploy` and
  `stellar contract invoke` — the exact commands are in the
  [README](../README.md#network-deployment).

## Common flows

### 1. Deposit → borrow → read → repay → withdraw

The full lifecycle, including the 150% collateral rule, is runnable in
[`borrow-lifecycle.ts`](../packages/sdk/examples/borrow-lifecycle.ts):

```ts
await client.deposit({ user, asset: collateralAsset, amount: 100n * UNIT });
await client.borrow({ user, asset: borrowAsset, amount: 40n * UNIT,
                     collateralAsset, collateralAmount: 100n * UNIT });

const position = await client.getUserPosition(user);   // collateral, debt, health factor
await client.repay({ user, asset: borrowAsset, amount: 40n * UNIT });
await client.withdraw({ user, asset: collateralAsset, amount: 100n * UNIT });
```

Notes:

- Amounts are `bigint` in the asset's smallest unit; `UNIT` in the examples is
  `10n ** 7n` for 7-decimal assets. `deposit`/`withdraw`/`repay`/`borrow`
  reject non-positive amounts client-side (`RangeError`) and the contract
  rejects them too (`InvalidAmount`, #4).
- The borrow must leave the position above the minimum collateral ratio — the
  lending contract currently enforces 15,000 bps (150%). A failing borrow
  returns `InsufficientCollateral` (#1).
- `borrow` takes both the borrowed asset and the collateral being pledged;
  repeated calls accumulate a multi-asset position up to the contract's
  collateral-asset cap (`TooManyCollateralAssets`, #14).

### 2. Reading position state

```ts
const position = await client.getUserPosition(user);  // collateral/debt values + health factor
const debt = await client.getUserDebt(user);          // principal + accrued interest
```

Health factor is scaled so **10,000 = 1.0**; positions at or below it are
liquidatable. Anything else on the contract is reachable through the escape
hatch: `client.invoke('entry_point', args, /* readonly */ true)`.

### 3. Liquidation

```ts
await client.liquidate({ liquidator, borrower, debtAsset, collateralAsset, amount });
```

- The position must be unhealthy (health factor below 10,000) — otherwise the
  call reverts with `PositionHealthy` (#10).
- A borrower cannot liquidate their own position (`SelfLiquidation`, #15).
- The liquidator receives collateral plus the configured incentive, capped so
  the incentive stays in bounds (`IncentiveOutOfBounds`, #17). Read
  [`liquidation.md`](liquidation.md) before automating liquidations.

### 4. Flash loans

Flash loans are atomic: your receiver contract is called inside the borrow
transaction and must repay the principal plus the configured fee before the
transaction ends. See
[`flash_loan.md`](flash_loan.md) for the receiver interface and
`stellar-lend/contracts/flash-loan` for the contract. Reentrancy into the
lending contract is detected and rejected (`ReentrancyDetected`, #12).

### 5. Cross-asset positions

Collateral and debt can be different assets, valued through oracle prices. The
position's health factor covers every collateral asset at once — see
[`cross_asset.md`](cross_asset.md) and `cross_asset_rules`.

### 6. Pre-flight checks before enabling a borrow form

Read the utilization circuit breaker so your UI does not offer borrows the
protocol will throttle:

- [`utilization-breaker.ts`](../packages/sdk/examples/utilization-breaker.ts)
  shows the read;
- a throttled borrow fails with `UtilizationThrottled` (#13).

Also check the pause switches (for example withdraws can be paused
independently) and surface that state instead of letting the call fail — see
[`pause.md`](pause.md).

## Errors

Normalize anything thrown by an adapter with the SDK helper; branch on `code`,
use `message` for display only, and retry only when `retryable` is true:

```ts
import { normalizeProtocolError } from '@stellarlend/sdk';

try {
  await client.borrow({ /* … */ });
} catch (error) {
  const normalized = normalizeProtocolError(error);
  if (normalized.retryable) scheduleRetry();
  showToast(normalized.message);
}
```

See [`error-handling.ts`](../packages/sdk/examples/error-handling.ts) for the
full mapping, and [`docs/TROUBLESHOOTING.md`](TROUBLESHOOTING.md) for turning a
raw `Error(Contract, #N)` into its cause.

## Events

Every state change publishes events (`deposit`, `borrow`, `repay`, `withdraw`,
`liquidate`, pause changes, and so on). Topic layouts and decoding examples are
in [`event-indexing.md`](event-indexing.md) and [`event-schema.md`](event-schema.md).

## Integrator security checklist

- **Simulate before you submit.** Every adapter example simulates first; the
  simulation is where auth-tree and balance errors surface cheaply. Handle a
  changed simulation result by rebuilding, never by force-submitting.
- **Auth trees.** Sign every entry the simulation returns for sponsored or
  delegated calls; do not assume a single signature suffices.
- **Oracle freshness.** Valuation depends on oracle prices; check the cache TTL
  configuration and handle `PriceUnavailable` (#16) as a retryable outage, not a
  user error.
- **Secrets stay server-side.** Use `keypairSigner` only in servers/bots;
  browsers must sign through the wallet adapter.
- **Respect rate limits.** Aggressive polling or retry storms are rate-limited
  (see [`mev-protection.md`](mev-protection.md)); back off on 429s.

## Reference

| Resource | What it covers |
| --- | --- |
| [`packages/sdk/README.md`](../packages/sdk/README.md) | SDK install, error model, handling strategy |
| [`packages/sdk/examples/README.md`](../packages/sdk/examples/README.md) | Every example file and the quick start |
| [`docs/README.md`](README.md) | Full documentation index |
| [`docs/GLOSSARY.md`](GLOSSARY.md) | Protocol terminology and fixed-point scales |
