# @stellarlend/sdk examples

Runnable, type-checked examples for common operations. Each file exports plain functions
so you can copy one into your app or call it directly. `examples.test.ts` covers every
example, and CI compiles and runs it.

| File | Shows |
|---|---|
| [`rpc-adapter.ts`](rpc-adapter.ts) | A production `ContractAdapter` over Soroban RPC: build, simulate, sign, submit, poll. Also `keypairSigner` for servers and bots. |
| [`borrow-lifecycle.ts`](borrow-lifecycle.ts) | Deposit → borrow → read position → repay → withdraw, plus the 150% collateral rule |
| [`error-handling.ts`](error-handling.ts) | Mapping `ProtocolError` codes to UI copy, retrying only `retryable` errors, recovering raw contract codes |
| [`wallet.ts`](wallet.ts) | Connecting a browser wallet (Freighter-style) with `WalletManager` and signing lending transactions through it |
| [`utilization-breaker.ts`](utilization-breaker.ts) | Reading the utilization circuit breaker to cap a borrow form before submitting |
| [`support/fake-adapter.ts`](support/fake-adapter.ts) | An in-memory adapter for unit-testing your own code against `StellarLendClient` |

## Quick start

```ts
import { StellarLendClient } from '@stellarlend/sdk';
import { SorobanRpcAdapter, keypairSigner } from '@stellarlend/sdk/examples/rpc-adapter';
import { runBorrowLifecycle, UNIT } from '@stellarlend/sdk/examples/borrow-lifecycle';

const adapter = new SorobanRpcAdapter({
  rpcUrl: 'http://localhost:8000/rpc',                    // npm run devnet
  networkPassphrase: 'Standalone Network ; February 2017',
  source: ALICE_ADDRESS,
  signTransaction: keypairSigner(ALICE_SECRET),           // stellar keys show sl-alice
});
const client = new StellarLendClient({ contractId: LENDING_ID, adapter });

await runBorrowLifecycle(client, {
  user: ALICE_ADDRESS, collateralAsset: XLM_ID, borrowAsset: USDC_ID,
  deposit: 100n * UNIT, borrow: 40n * UNIT,
});
```

Contract ids for the local devnet are in `.devnet/state.json` after `npm run devnet`.
The examples ship as TypeScript source in the published package, so import them through
a TypeScript-aware bundler or copy them into your project.

## Develop

```bash
npm run test:examples -w @stellarlend/sdk   # type-check src + examples, then run the tests
npm run typecheck -w @stellarlend/sdk
```
