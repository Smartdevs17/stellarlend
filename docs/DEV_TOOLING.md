# Developer Tooling Guide

This guide covers the local mainnet-derived development workflow, runtime trace analysis, deployment verification, and mutation testing additions introduced for the devtooling issues.

## Local Mainnet-Derived Sandbox

Stellar Quickstart supports `local`, `testnet`, `futurenet`, and `pubnet` modes. It does **not** support a mutable pubnet fork in the same way an EVM fork does. The local StellarLend workflow therefore uses:

1. `pubnet` as the read-only reference network for realistic assumptions.
2. `local` as the writable sandbox for contract deployment, hot-reload, and state manipulation.
3. `scripts/dev/default-seed.json` as the compatibility layer between the reference network and the writable sandbox.

### Start the sandbox

```bash
npm run dev:fork
```

Useful flags:

```bash
bash scripts/dev/start-fork-env.sh --source-network testnet --reset
bash scripts/dev/start-fork-env.sh --seed-file scripts/dev/default-seed.json
```

The script writes `.dev/fork.env` so tooling can reuse the selected source network and seed manifest.

### Hot reload contracts

```bash
npm run dev:watch:contracts
```

If you want every rebuild redeployed into the local sandbox:

```bash
bash scripts/dev/watch-contracts.sh --deploy-local
```

This requires `cargo-watch`:

```bash
cargo install cargo-watch
```

### Manipulate fork-specific state

```bash
npm run dev:seed
```

This generates `.dev/seed.env` from `scripts/dev/default-seed.json`. If `ADMIN_SECRET_KEY` and `LENDING_CONTRACT_ID` are exported, the helper also applies the emergency pause flag on-chain so pause-path debugging can be exercised in the sandbox.

### Network switching

- `pubnet` reference: `bash scripts/dev/start-fork-env.sh --source-network pubnet`
- `testnet` reference: `bash scripts/dev/start-fork-env.sh --source-network testnet`
- `futurenet` reference: `bash scripts/dev/start-fork-env.sh --source-network futurenet`

The sandbox itself always runs in `local` mode so contracts remain writable and disposable.

### Performance notes

- `pubnet` reference mode uses substantially more CPU, RAM, and disk than `testnet` or `local`.
- The sandbox is intended for focused contract debugging, not soak testing.
- Keep trace collection scoped to a single transaction or contract family; full-session tracing is noisy and expensive.

## Runtime Verification and Contract Tracing

The oracle workspace now ships a trace analysis helper that can summarize nested contract invocations, gas breakdown, state changes, and tracing overhead.

### Analyze a trace capture

```bash
npm run trace:contract -- path/to/trace.json
```

Input format:

```json
{
  "transactionHash": "...",
  "network": "local",
  "ledger": 123,
  "elapsedMs": 12,
  "tracingElapsedMs": 16,
  "invocations": [
    {
      "contractId": "CLEND",
      "functionName": "borrow",
      "gasUsed": 12,
      "children": []
    }
  ]
}
```

The analyzer returns:

- flattened call frames suitable for call-stack reconstruction
- cumulative gas hot paths
- aggregated state-change summaries
- overhead warnings when tracing materially changes runtime characteristics

## Lending Pool Position Health Simulation

Simulate market conditions against the lending pool's own health factor and
liquidation threshold math, before a price move happens on-chain.

```bash
# A shipped example position, through a flash crash
node --experimental-strip-types scripts/position-health-sim/index.ts \
  --position leveraged-xlm-borrow --grid flash_crash

# One historical scenario, at an explicit threshold
node --experimental-strip-types scripts/position-health-sim/index.ts \
  --position healthy-xlm-borrow --scenario luna-ust-collapse --threshold-bps 8500

# What scenarios and shock grids exist?
node --experimental-strip-types scripts/position-health-sim/index.ts --list-scenarios

# Gate: fail if any market condition liquidates the position
node --experimental-strip-types scripts/position-health-sim/index.ts \
  --position leveraged-xlm-borrow --grid flash_crash --fail-on liquidatable
```

The health factor is a port of `views.rs::compute_health_factor`, and the
constants it depends on (`HEALTH_FACTOR_SCALE`, `PRICE_SCALE`,
`HEALTH_FACTOR_NO_DEBT`, `COLLATERAL_RATIO_MIN` and the `borrow.rs` admin
defaults) are **parsed out of the contract source** rather than copied — so
renaming one on-chain makes the tool fail loudly instead of simulating against a
stale number. Use `--contract-src` to point it at a different checkout or
revision.

This matters because the off-chain health-factor surfaces disagree with the
contract: `risk-engine/stress-tester/engine.ts`, `services/risk-simulation.service.ts`,
`services/position-simulator.ts` and `controllers/simulation.controller.ts` all
compute a raw `collateralValue / debtValue` with **no liquidation threshold**, and
`portfolio.service.ts` hardcodes 1.2 where the contract's default is 0.8. At the
default 80% threshold the contract's health factor is 0.8× the raw ratio, so those
surfaces call positions healthy that the contract would liquidate. This tool is
the reference to check them against.

It also answers the threshold question the repo could not: `liquidation_threshold_bps`
is admin-settable across 1..10000 bps, and `--thresholds` (or the default grid)
shows what a change does to the liquidation boundary.

Note that `liquidate` in the lending pool is still a stub (`lib.rs:298`), and
neither `close_factor_bps` nor `liquidation_incentive_bps` is exposed in the
`#[contractimpl]`, so the tool prices what a liquidation *would* cost from the
view functions rather than from a liquidation path.

See `scripts/position-health-sim/README.md` for the position file format, the
fidelity details, and the tests transcribed from the contract's own suite.

## Deployment Verification

After deployment and initialization, run:

```bash
npm run deploy:verify -- --network testnet
```

The verifier checks:

- lending admin readability
- risk parameter defaults
- emergency pause state
- local artifact hash against the deployment manifest
- AMM settings readability when an AMM deployment is present

## Contract Source Code Verification

To verify that deployed contracts match their source code:

```bash
# Verify a specific contract
npm run verify:contract -- --contract-id <contract_id> --source <source_path> --network testnet

# Verify during deployment
npm run deploy -- --network testnet --build --verify
```

The verification rebuilds the contract from source and compares the bytecode with the deployed contract. This ensures:

- Automated compilation verification
- Metadata matching
- Multi-file verification support
- Proxy pattern verification (for upgradeable contracts)

Verification results are stored in the deployment manifest with a `verification_status` field.

## Mutation Testing

Mutation testing is configured in the API workspace with Stryker.

The initial mutation gate targets the pagination cursor helpers (`src/utils/pagination.ts`) because that slice has a clean, deterministic Jest baseline today. The scope can be widened as the rest of the API test suite is stabilized.

### Run locally

```bash
npm run mutation:api
```

### Expected quality gate

- high: 80
- low: 70
- break: 70

Mutation reports are written to `api/reports/mutation/`.

### Current baseline

- Latest local run on this branch: 78.87 mutation score for `src/utils/pagination.ts`
- Surviving mutants show the highest-value follow-up tests are:
  - assert the exact thrown messages for malformed and empty cursor cases
  - cover the `parsedLimit > maxLimit` rejection path explicitly
  - verify `decodeCursor` rejects empty decoded payloads more directly
  - verify cursor trimming remains part of the opaque cursor contract

### CI strategy

The repository includes a dedicated GitHub Actions workflow for scheduled or manually triggered mutation runs so regular PR validation remains fast while mutation testing still has a persistent quality gate.