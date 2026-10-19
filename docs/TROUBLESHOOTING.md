# Troubleshooting Guide

Common problems when building, testing, or integrating StellarLend, with the
commands and checks that resolve them. Search this page for the exact error text
or symptom you saw — headings and error names are written to be greppable.

## Symptom index

| Symptom | Jump to |
| --- | --- |
| `stellar: command not found`, unknown `wasm32` target | [Build and toolchain](#build-and-toolchain) |
| `npm ci`/`npm install` fails, module not found in a workspace | [Node workspaces](#node-workspaces) |
| Contract tests fail to compile claiming missing `testutils` | [Tests](#tests) |
| Jest cannot find modules or fails on import | [Tests](#tests) |
| Devnet containers exited, ports busy, healthcheck never passes | [Local devnet](#local-devnet) |
| `Error(Contract, #N)` from a contract call | [Contract errors](#contract-errors) |
| `PriceUnavailable` or oracle returns stale values | [Oracle prices](#oracle-prices) |
| Auth/signature errors, `tx too late`, sequence numbers | [Transactions](#transactions) |
| API returns 5xx or rate-limits | [API service](#api-service) |

## Build and toolchain

- **`wasm32-unknown-unknown` target missing.** Install it once:

  ```bash
  rustup target add wasm32-unknown-unknown
  ```

- **`stellar: command not found`.** Install the current [stellar-cli](https://github.com/stellar/stellar-cli);
  the quickstart image and `docker/devnet` already include it.

- **Node version errors.** The root `engines` floor is Node 20 (`package.json`).
  Check with `node --version`; use a version manager (`nvm use 20`) if you are
  below it. Errors mentioning syntax or `??=` on old versions come from this.

- **Compilation errors after switching branches.** Clean the artifacts:
  `cargo clean` (Rust) and `npm run clean` where defined; stale incremental
  artifacts across toolchain versions are a common cause.

## Node workspaces

- **Always install from the repository root.** `api`, `oracle`, `packages/sdk`,
  and `packages/ui-lending` are npm workspaces; running `npm install` inside a
  subdirectory produces a half-linked tree.

- **Lockfile drift.** Use `npm ci` for a reproducible install; `npm install`
  when you are intentionally changing dependencies.

- **"Cannot find module '@stellarlend/…'".** Remove the workspace-installed
  links and reinstall from root:

  ```bash
  rm -rf node_modules api/node_modules oracle/node_modules packages/*/node_modules
  npm install
  ```

- **TypeScript path errors after a package rename.** Run `npm run build:all`
  first — some workspaces consume each other's `dist/` output.

## Tests

- **Contract tests fail to compile with missing test helpers.** The suites use
  the shared `test-utils` framework behind a feature flag:

  ```bash
  cargo test --all --features testutils   # from the workspace root
  scripts/testing/run-contract-tests.sh   # the same thing, with reporting
  ```

- **A single suite is slow or flaky under the full run.** Run it alone with the
  exact test name (`cargo test -p lending <name> -- --exact`). Note which suite
  hangs and include it in your issue.

- **Jest failures on a fresh clone.** Install from root first, then confirm the
  version floor (`node --version`). Tests read `.env` files — copy
  `.env.example` to `.env` in the relevant workspace (`api/.env`,
  `oracle/.env`) before running.

- **Devnet tests time out.** The devnet must be up and healthy first:

  ```bash
  cd docker/devnet
  docker compose up -d
  ./healthcheck.sh --wait
  npm run test:devnet     # from the repo root
  ```

## Local devnet

- **Containers exit immediately.** Read the service logs rather than
  restarting:

  ```bash
  docker compose logs --tail=100 api oracle postgres
  ```

  The most common cause is a missing `docker/devnet/.env` (copy it from
  `.env.example`).

- **Port already in use.** The default stack uses 8000 (Soroban RPC/Horizon),
  3000 (API), 5432 (Postgres), and 6379 (Redis). Stop the conflicting process or
  remap the port in `docker-compose.yml`.

- **Leftover state from an older run.** Reset everything; the bootstrap script
  can also do it:

  ```bash
  npm run devnet:reset
  ```

- **Need funds for a local account.** Use Friendbot:
  `curl "http://localhost:8000/friendbot?addr=<ADDRESS>"`.

## Contract errors

Soroban surfaces contract failures as `Error(Contract, #N)`. `N` is the
discriminant of the contract's error enum — find it in the module you called
(for example `stellar-lend/contracts/lending/src/borrow.rs`). The lending
borrow path's code map is:

| # | Code | Meaning and first thing to check |
| --- | --- | --- |
| 1 | `InsufficientCollateral` | Below the 150% minimum ratio (15,000 bps). Add collateral or reduce the borrow. |
| 2 | `DebtCeilingReached` | Protocol-wide debt ceiling hit; the admin must raise it before new borrows. |
| 3 | `ProtocolPaused` | Borrows are paused; check the pause switches (`PauseType`). |
| 4 | `InvalidAmount` | Zero or negative amount — amounts must be positive integers. |
| 5 | `Overflow` | Arithmetic overflow; report it with the exact inputs, it is a bug if reachable. |
| 6 | `Unauthorized` | Caller is not the position owner/delegate; sign with the right source. |
| 7 | `AssetNotSupported` | The borrow asset is not enabled; add it via the admin flow first. |
| 8 | `BelowMinimumBorrow` | Borrow is under the configured dust/minimum; borrow a larger amount. |
| 9 | `RepayAmountTooHigh` | Repaying more than the outstanding debt; repay the exact remaining amount. |
| 10 | `PositionHealthy` | Liquidation attempted on a healthy position (health factor above 10,000). |
| 11 | `InsufficientReserves` | Bad-debt recovery lacks reserves. |
| 12 | `ReentrancyDetected` | The token/receiver re-entered; use a standard token and retry. |
| 13 | `UtilizationThrottled` | Utilization circuit breaker; wait for the window to roll or for utilization to drop. |
| 14 | `TooManyCollateralAssets` | Position holds the maximum distinct collateral assets. |
| 15 | `SelfLiquidation` | The borrower cannot liquidate their own position; use a different liquidator. |
| 16 | `PriceUnavailable` | Oracle has no usable price for a required asset — see below. |
| 17 | `IncentiveOutOfBounds` | Liquidation incentive outside the allowed bounds; admin configuration issue. |

Other contracts name their enums the same way (`*Error` in the same module as
the failing entry point).

## Oracle prices

- **`PriceUnavailable` / `NoPrice` on a call.** The price cache has no live
  entry for that asset: confirm the asset is registered, the oracle was
  configured for it, and its cache TTL (`set_price_cache_ttl`) has not lapsed.

- **Prices look stale.** The cache serves the last accepted price until the TTL
  expires; a stale price still within TTL is expected. Force a refresh by
  triggering the oracle update path, then re-read.

- **A plausible price was rejected.** The aggregation pipeline drops outliers
  (Z-score/IQR/velocity checks) and TWAP smoothing damps single-tick spikes.
  A rejected report is normal behavior, not a failure — check the oracle logs
  and see [REALTIME_PRICE_FEED.md](REALTIME_PRICE_FEED.md).

## Transactions

- **`tx too late` / expired transaction.** Rebuild the transaction; Soroban
  transaction time bounds are short.

- **`account not found` / sequence mismatch.** The source account is unfunded
  or its sequence is stale. Fund it (Friendbot on testnet), re-fetch the
  account, and resubmit. Do not retry a signed transaction verbatim after a
  timeout — rebuild and re-sign.

- **Missing signature / auth tree errors.** `require_auth` must be satisfied by
  the right source account or an explicit auth entry. When a delegated or
  sponsored flow is involved, sign every entry in the auth tree the simulation
  returns; see [multisig.md](multisig.md) and the SDK's auth helpers.

## API service

- **Startup failures or 500s.** Copy `api/.env.example` to `api/.env` and set
  the database/Redis values for your environment, then restart. The first
  request after boot can fail while the DB pool warms — retry once before
  filing an issue.

- **429s or requests rejected under load.** The API is rate-limited (see
  `contracts/rate-limit` and [mev-protection.md](mev-protection.md)); check
  `Retry-After` and back off instead of retrying immediately.

## Still stuck?

Open an issue with:

1. The exact command you ran and its full output (not a summary).
2. Versions: `node --version`, `rustc --version`, `stellar --version`, and the
   commit hash (`git rev-parse --short HEAD`).
3. For contract errors: the entry point, arguments, and the `Error(Contract,
   #N)` value.
4. For devnet issues: the output of `docker compose ps` and the relevant
   service logs.

That checklist usually cuts the round-trip in half.
