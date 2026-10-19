# StellarLend examples

The one place to find StellarLend examples. This workspace (`@stellarlend/examples`)
holds runnable apps built on the public `@stellarlend/sdk` API, and the tables below
index every other example in the repository.

It depends on the SDK by version, not by path, so it builds the same way when copied
out of the monorepo.

## Run

```bash
npm install                                   # from the repo root, once
npm test -w @stellarlend/examples             # build SDK + examples, run the tests (no network)

npm run devnet                                # local devnet with seeded positions
npm run devnet:faucet -- sl-dave              # optional: fund an extra account
npm run build -w @stellarlend/examples
npm start -w @stellarlend/examples            # read-only tour against the devnet
```

`npm start` reads `.devnet/state.json` (or `$DEVNET_STATE_DIR/state.json`), prints
the lending contract's recent events and the review screen for a leverage plan. It
never signs or submits anything.

## In this workspace

| File | Shows |
|---|---|
| [`src/events.ts`](src/events.ts) | Auto-pagination: streaming every contract event from Soroban RPC `getEvents` with `paginate`, `maxItems` and lazy page fetching |
| [`src/review-queue.ts`](src/review-queue.ts) | Typed transaction builders: building a deposit + borrow plan with `client.tx`, rendering it for confirmation, executing it with `client.execute` |
| [`src/devnet.ts`](src/devnet.ts) | Loading network settings and contract ids from the local devnet |
| [`src/main.ts`](src/main.ts) | Runnable entry point tying the above together |
| [`test/examples.test.ts`](test/examples.test.ts) | Tests for every example, against stubbed RPC and adapters |

## Elsewhere in the repo

| Where | What | Tested |
|---|---|---|
| [`packages/sdk/examples/`](../packages/sdk/examples/README.md) | SDK reference examples shipped inside the npm package: Soroban RPC adapter, borrow lifecycle, error handling, wallet connection, utilization breaker, in-memory test adapter | yes (`npm test -w @stellarlend/sdk`) |
| [`docs/examples/javascript.md`](../docs/examples/javascript.md) | Calling the REST API from JavaScript/TypeScript | no (snippets) |
| [`docs/examples/python.md`](../docs/examples/python.md) | Calling the REST API from Python | no (snippets) |
| [`docs/examples/*.json`](../docs/examples/) | Example protocol and user report payloads | n/a |
| [`docs/devnet.md`](../docs/devnet.md) | CLI walkthrough against the local devnet, including the faucet | dry-run plan tests (`npm run test:devnet`) |
| [`docs/CONTRACT_INTEGRATION.md`](../docs/CONTRACT_INTEGRATION.md) | End-to-end contract integration flows | no (guide) |

## Adding an example

1. Add `src/<name>.ts` exporting plain functions that take their dependencies (client,
   server, config) as arguments, so they can be tested with stubs.
2. Cover it in `test/examples.test.ts`.
3. Add a row to the table above.

CI runs `npm test -w @stellarlend/examples`, so an example that stops compiling against
the SDK fails the build.
