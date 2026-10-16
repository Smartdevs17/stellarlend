# Local Devnet

One command gives you a local Stellar network with the lending contract deployed,
initialized and seeded with positions:

```bash
npm run devnet
```

## Prerequisites

| Tool | Version | Why |
|---|---|---|
| Docker | any recent | runs the `stellar/quickstart` node |
| stellar CLI | **≥ 25.2.0** | soroban-sdk 27 contracts only build with `stellar contract build` from 25.2+ |
| Rust + `wasm32v1-none` target | stable | `rustup target add wasm32v1-none` |
| curl | any | friendbot funding, health checks |

Install the CLI with `cargo install --locked stellar-cli`, or see the
[Stellar docs](https://developers.stellar.org/docs/tools/cli).

## What it does

`scripts/devnet/bootstrap.sh`:

1. Starts a quickstart node with `stellar container start local --limits unlimited` (RPC
   at `http://localhost:8000/rpc`). The `unlimited` limits let the full lending wasm deploy.
2. Creates and funds identities `sl-admin`, `sl-alice`, `sl-bob` and `sl-carol` through friendbot.
3. Builds and deploys the lending contract.
4. Resolves the native XLM asset contract and deploys a `USDC:<admin>` asset contract.
5. Initializes the protocol (1,000,000 USDC debt ceiling, 1 USDC minimum borrow) and seeds:

   | user | positions |
   |---|---|
   | alice | 50,000 XLM vault deposit; 25,000 USDC borrow against 50,000 XLM (200%) |
   | bob | 40,000 USDC borrow against 60,000 XLM (exactly the 150% minimum) |
   | carol | 20,000 XLM deposit; 5,000 USDC borrow against 10,000 XLM; 2,000 USDC repaid |

6. Enables the [utilization circuit breaker](UTILIZATION_CIRCUIT_BREAKER.md) (trip 90%,
   reset 80%, 10,000 USDC per borrow while tripped).
7. Prints each position read back from the contract, then writes `.devnet/state.json`:

```json
{
  "network": { "rpcUrl": "...", "networkPassphrase": "...", "friendbotUrl": "..." },
  "contracts": { "lending": "C...", "lendingWasmHash": "...", "xlm": "C...", "usdc": "C..." },
  "accounts": { "admin": { "identity": "sl-admin", "address": "G..." }, "...": {} }
}
```

Running it again against a live devnet does nothing except print the summary.

## Commands

| Command | Effect |
|---|---|
| `npm run devnet` | Bootstrap, or print the summary if already up |
| `npm run devnet:reset` | Stop the node, delete `.devnet/`, bootstrap from an empty ledger |
| `npm run devnet:rehearse-upgrade` | Rehearse an upgrade and rollback ([runbook](runbooks/upgrade-and-migration.md)) |
| `bash scripts/devnet/bootstrap.sh --dry-run` | Print every command without running anything |
| `bash scripts/devnet/bootstrap.sh --no-node` | Use an RPC you already run (e.g. `docker/devnet` compose) |
| `bash scripts/devnet/bootstrap.sh --skip-build` | Deploy the existing wasm |
| `npm run test:devnet` | Plan tests for the scripts (no network needed) |

Environment overrides: `DEVNET_RPC_URL`, `DEVNET_FRIENDBOT_URL`, `DEVNET_PASSPHRASE`,
`DEVNET_CONTAINER`, `DEVNET_KEY_PREFIX`, `DEVNET_STATE_DIR`, `DEVNET_WASM`,
`DEVNET_HEALTH_TIMEOUT`.

## Using it

```bash
LENDING=$(grep -oE '"lending": "[^"]+"' .devnet/state.json | cut -d'"' -f4)
USDC=$(grep -oE '"usdc": "[^"]+"' .devnet/state.json | cut -d'"' -f4)
NET=(--rpc-url http://localhost:8000/rpc --network-passphrase "Standalone Network ; February 2017")

# read (simulated, free)
stellar contract invoke --id "$LENDING" --source sl-alice "${NET[@]}" --send=no -- \
  get_user_position --user "$(stellar keys address sl-alice)"

# write: bob repays 1 USDC
stellar contract invoke --id "$LENDING" --source sl-bob "${NET[@]}" -- \
  repay --user "$(stellar keys address sl-bob)" --asset "$USDC" --amount 10000000
```

From TypeScript, point `SorobanRpcAdapter` from the
[SDK examples](../packages/sdk/examples/README.md) at `state.json`'s `rpcUrl` and
`contracts.lending`.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `soroban-sdk requires stellar-cli v25.2.0+` | Upgrade the CLI. Plain `cargo build` is refused by the SDK. |
| `RPC not healthy after 180s` | First boot downloads the image; raise `DEVNET_HEALTH_TIMEOUT` or check `docker logs stellar-local` |
| Deploy fails with a size or resource limit | The node was started without `--limits unlimited`; run `npm run devnet:reset` |
| Stale contract ids after restarting Docker | Quickstart's local ledger is ephemeral: run `npm run devnet:reset` |

## Relation to `docker/devnet`

`docker/devnet/` is the full-stack compose environment (API, oracle, Postgres, Redis).
To run the full stack on top of a contract deployment from this script, start the compose
stack and bootstrap against its node:

```bash
(cd docker/devnet && docker compose up -d soroban-devnet)
DEVNET_RPC_URL=http://localhost:8000/soroban/rpc bash scripts/devnet/bootstrap.sh --no-node
```
