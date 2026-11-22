# StellarLend CLI (`sllend`)

Developer CLI for the StellarLend lending protocol on Stellar / Soroban, rewritten using `clap`'s type-safe subcommand derive macros (`#[derive(Parser, Subcommand, Args)]`).

## Features

- **Type-safe Command Hierarchy**: Declarative subcommand routing and argument parsing via `clap` derive.
- **Protocol Operations**: `deposit`, `withdraw`, `borrow`, `repay`, `liquidate`.
- **Protocol Telemetry**: `get-position` (`position`), `get-pool` (`pool`).
- **Keystore Management**: `keys add`, `keys list`, `keys remove`.
- **Configuration Management**: `config show`, `config set-network`, `config set-contract`, `config set-default-network`.
- **Batch Processing**: Run automated command sequences via JSON files (`sllend batch commands.json`).
- **Interactive REPL**: Explore and execute commands interactively (`sllend interactive` or `sllend i`).
- **Observability**: Built-in structured logging with `tracing` and verbosity flags (`-v`, `-vv`).
- **Scripting & CI**: First-class `--json` mode for programmatic integration.

## Installation & Building

```bash
cd cli
cargo build --release
```

The resulting binary will be at `target/release/sllend`.

To install to your `PATH`:
```bash
cargo install --path .
```

## Global Options

| Flag | Env Variable | Description |
|------|--------------|-------------|
| `--network <NAME>` | `SLLEND_NETWORK` | Network profile to target (`testnet`, `mainnet`, `local`) |
| `--contract <ID>` | `SLLEND_CONTRACT` | Override StellarLend contract address |
| `--key <ALIAS>` | `SLLEND_KEY` | Signing key alias from local keystore |
| `--secret <KEY>` | `SLLEND_SECRET` | Raw Stellar secret key (`S...`) |
| `--account <ADDR>` | `SLLEND_ACCOUNT` | Override sender public key (`G...`) for queries |
| `--rpc-url <URL>` | `SLLEND_RPC_URL` | Override Soroban RPC endpoint |
| `--json` | - | Output structured JSON for automation |
| `-v`, `-vv` | - | Verbosity level (Debug, Trace) |

## Command Reference

### Lending Operations

- **Deposit Collateral**
  ```bash
  sllend deposit 100.5
  sllend deposit 100 --asset CDUMMY_TOKEN_ID
  ```

- **Withdraw Collateral**
  ```bash
  sllend withdraw 50
  sllend withdraw 50 --asset CDUMMY_TOKEN_ID
  ```

- **Borrow Assets**
  ```bash
  sllend borrow 25.0
  sllend borrow 25 --asset CDUMMY_TOKEN_ID
  ```

- **Repay Debt**
  ```bash
  sllend repay 10.0
  sllend repay max
  ```

- **Liquidate Position**
  ```bash
  sllend liquidate GBORROWER_ADDRESS 50.0 --collateral-asset CCOL --debt-asset CDEBT
  ```

### Telemetry & Queries

- **Get Position**
  ```bash
  sllend get-position
  sllend get-position GACCOUNT_ADDRESS --json
  ```

- **Get Pool Statistics**
  ```bash
  sllend get-pool
  sllend get-pool --json
  ```

### Keystore & Configuration

- **Key Management**
  ```bash
  sllend keys add alice SXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
  sllend keys list
  sllend keys remove alice
  ```

- **Configuration**
  ```bash
  sllend config show
  sllend config set-default-network testnet
  sllend config set-contract testnet CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
  ```

### Batch & Interactive

- **Batch Execution**
  ```bash
  sllend batch commands.json
  ```
  Example `commands.json`:
  ```json
  [
    { "cmd": "deposit", "args": ["100"] },
    { "cmd": "borrow", "args": ["20"] },
    { "cmd": "get-position", "args": [] }
  ]
  ```

- **Interactive REPL**
  ```bash
  sllend interactive
  ```

## Running Tests

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
