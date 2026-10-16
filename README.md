## Stablecoin Reserve Rebalancing Automation

The system automatically maintains stablecoin pegs across pools by:

1. **Monitoring**: Continuously checks pool prices against target peg
2. **Detecting**: Identifies significant deviations (>1% by default)
3. **Arbitrage**: Executes cross-pool transfers to normalize prices
4. **Safety**: Maintains minimum liquidity ratios and transfer limits

### Configuration

Edit `src/config/automation.ts`:

```typescript
const stablecoinConfig = {
  targetPegPrice: 1.0,          // Target price (e.g., 1.0 for USDT/USDC)
  thresholdPercentage: 0.01,    // % deviation before action
  maxTransferAmount: 1000000,   // USD value per transfer
  minLiquidityRatio: 0.95,      // Minimum liquidity ratio
  checkIntervalMinutes: 15,    // Monitoring frequency
  pools: ['USDT_POOL_1', 'USDC_POOL_2'] // Whitelisted pools
};
```

### Troubleshooting

- **No actions executed**: Verify pool prices are outside threshold
- **Failed transfers**: Check liquidity ratios and transaction limits
- **Monitoring gaps**: Confirm scheduler is running (`src/scheduler.ts`)

### Testing

Run comprehensive tests:
```bash
npm test automation/balancer.test.ts
```

<<<<<<< HEAD
The unified contract suite runs the shared `test-utils` framework and the suites built on it. It records timing, gas benchmarks and coverage:

```bash
scripts/testing/run-contract-tests.sh            # all suites
scripts/testing/run-contract-tests.sh bench      # gas benchmark reports
scripts/testing/run-contract-tests.sh coverage   # lcov + summary (cargo-llvm-cov)
```

See [stellar-lend/TESTING.md](stellar-lend/TESTING.md) for fixtures, seeding, scenarios and CI.

### Running Local CI Checks

To reproduce CI checks locally before pushing:

```bash
# From project root
chmod +x local-ci.sh
./local-ci.sh
```

This script runs:
- Format checking (`cargo fmt`)
- Linting (`cargo clippy`)
- Contract building and optimization
- Unit tests
- Security audit (`cargo audit`)
- Documentation generation

## Docker Quickstart

To run the project locally without manually installing Node.js dependencies, you can use Docker:

1. Ensure you have [Docker](https://docs.docker.com/get-docker/) and Docker Compose installed.
2. Create your local environment files at `api/.env` and `oracle/.env` (use `.env.example` as a template).
3. From the root directory, build and start the containers:
   ```bash
   docker-compose up --build
   ```
4. The API will be accessible at http://localhost:3000.

### Local Soroban Devnet (Docker Compose)

For a full local Stellar/Soroban environment (local node, Horizon, Friendbot,
deployed contracts, seeded accounts and test data):

```bash
cd docker/devnet
cp .env.example .env
docker compose up -d            # boots the Soroban node, API, oracle, postgres, redis
./healthcheck.sh --wait         # wait for init + services
```

- Soroban RPC: http://localhost:8000/soroban/rpc
- Horizon: http://localhost:8000
- Friendbot (free test XLM): http://localhost:8000/friendbot

See [docker/devnet/README.md](docker/devnet/README.md) for full setup, seeded
data, liquidation scenarios, and reset instructions. The same environment is
used by the VS Code Dev Container.

### Network Deployment

#### Deploy to Testnet

```bash
# Build the contract
stellar contract build

# Deploy to testnet (requires testnet account)
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/hello_world.wasm \
  --network testnet \
  --source <your-testnet-keypair>

# Initialize the contract
stellar contract invoke \
  --id <contract-id> \
  --network testnet \
  --source <admin-keypair> \
  -- initialize \
  --admin <admin-address>
```

#### Deploy to Mainnet

```bash
# Build and optimize
stellar contract build
stellar contract optimize \
  --wasm target/wasm32-unknown-unknown/release/hello_world.wasm

# Deploy (use optimized WASM)
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/hello_world-optimized.wasm \
  --network mainnet \
  --source <your-mainnet-keypair>
```

**⚠️ Security Note**: Always audit and test thoroughly before deploying to mainnet. Use multisig for admin operations in production.

---

## Repository Structure

```
stellarlend-contracts/
├── README.md                 # This file
├── local-ci.sh               # Local CI reproduction script
├── ci-doc.md                 # CI/CD documentation
├── docs/                     # Protocol documentation
│   ├── README.md            # Detailed protocol documentation
│   └── examples/            # Example JSON reports
│       ├── protocol_report.json
│       └── user_report.json
└── stellar-lend/            # Main contract workspace
    ├── Cargo.toml           # Workspace configuration
    └── contracts/
        └── hello-world/     # Main StellarLend contract
            ├── Cargo.toml
            ├── Makefile     # Build/test shortcuts
            ├── README.md    # Contract-specific docs
            └── src/
                ├── lib.rs   # Main contract entry point
                ├── deposit.rs
                ├── borrow.rs
                ├── repay.rs
                ├── withdraw.rs
                ├── liquidate.rs
                ├── oracle.rs
                ├── governance.rs
                ├── amm.rs
                ├── flash_loan.rs
                ├── analytics.rs
                └── test.rs  # Test suite
```

---

## Contract Modules

The StellarLend contract is organized into the following modules:

- **Core Lending** (`deposit.rs`, `borrow.rs`, `repay.rs`, `withdraw.rs`): Deposit collateral, borrow assets, repay debt, and withdraw collateral
- **Liquidation** (`liquidate.rs`): Partial liquidation with close factor and liquidation incentives
- **Oracle** (`oracle.rs`): Price feed integration with validation, fallback, and caching
- **Governance** (`governance.rs`): Admin controls, multisig, and parameter management
- **AMM Integration** (`amm.rs`): Automated market maker hooks for swaps and liquidity
- **Flash Loans** (`flash_loan.rs`): Configurable flash loan functionality
- **Analytics** (`analytics.rs`): Protocol and user metrics, activity feeds, and reporting

---

## Key Entrypoints

### Core Operations

| Function                      | Description                                      |
|-------------------------------|--------------------------------------------------|
| `initialize`                  | Initialize contract and set admin                 |
| `deposit_collateral`          | Deposit collateral to the protocol                |
| `borrow`                      | Borrow assets against collateral                  |
| `repay`                       | Repay borrowed assets                            |
| `withdraw`                    | Withdraw collateral                              |
| `liquidate`                   | Liquidate undercollateralized positions          |

### Cross-Asset Operations

| Function                      | Description                                      |
|-------------------------------|--------------------------------------------------|
| `set_asset_params`            | Configure asset-specific parameters              |
| `deposit_collateral_asset`    | Deposit specific asset as collateral             |
| `borrow_asset`                | Borrow specific asset                            |
| `repay_asset`                 | Repay specific asset                             |
| `withdraw_asset`              | Withdraw specific asset                          |
| `get_cross_position_summary`  | Get unified position across all assets           |

### Admin & Configuration

| Function                      | Description                                      |
|-------------------------------|--------------------------------------------------|
| `set_risk_params`             | Set close factor and liquidation incentive       |
| `set_pause_switches`          | Pause/unpause protocol actions                   |
| `set_oracle`                  | Set oracle address                               |
| `set_min_collateral_ratio`    | Set minimum collateral ratio                    |
| `set_base_rate`               | Set base interest rate                          |
| `set_kink_utilization`        | Set kink utilization point                      |
| `set_multiplier`              | Set interest rate multiplier                    |
| `set_reserve_factor`          | Set protocol reserve factor                     |
| `set_rate_limits`             | Set interest rate floor/ceiling                 |
| `emergency_rate_adjustment`   | Emergency interest rate adjustment               |

### Query Functions

| Function                      | Description                                      |
|-------------------------------|--------------------------------------------------|
| `get_position`                | Query user position (collateral, debt, ratio)    |
| `get_protocol_params`         | Query protocol parameters                        |
| `get_risk_config`             | Query risk management configuration              |
| `get_system_stats`            | Query system-wide stats                          |
| `get_protocol_report`         | Get comprehensive protocol analytics             |
| `get_user_report`             | Get user-specific analytics                      |
| `get_recent_activity`         | Get activity feed                                |

For a complete list of entrypoints including AMM, flash loans, bridge, governance, and upgrade functions, see the [contract README](stellar-lend/contracts/hello-world/README.md) and [protocol documentation](docs/README.md).

---

## Documentation

- **[Protocol Documentation](docs/README.md)**: Comprehensive protocol documentation including modules, admin operations, monitoring, analytics, and upgrade procedures
- **[Upgrade Authorization](docs/UPGRADE_AUTHORIZATION.md)**: Strict upgrade authorization boundaries, key rotation workflow, and security assumptions
- **[Storage Layout and Migration](docs/storage.md)**: Detailed documentation of the contract's persistent storage structure, keys, types, and upgrade/migration strategies
- **[Fuzzing Strategy](docs/fuzzing.md)**: Coverage-guided, property-based fuzzing harnesses (cargo-fuzz/libFuzzer) for contract edge cases
- **[Contract README](stellar-lend/contracts/hello-world/README.md)**: Contract-specific documentation and entrypoint reference
- **[CI/CD Documentation](ci-doc.md)**: Continuous integration setup and local reproduction guide
- **[Example Reports](docs/examples/)**: Example JSON outputs for protocol and user analytics

---

##  Changelog
See [CHANGELOG.md](CHANGELOG.md) for a full list of updates and changes.


## Contributing

We welcome contributions! StellarLend participates in the **Stellar Wave Program** via [Drips](https://www.drips.network/). Contributors can earn points and rewards by picking up issues labeled **`Stellar Wave`** in the issue tracker.

Types of contributions we're looking for:

- **Contract improvements** — new features, optimizations, gas efficiency
- **Security audits** — reviewing contract logic, identifying vulnerabilities
- **Testing** — expanding test coverage, edge cases, fuzz testing, integration tests
- **Oracle enhancements** — additional price feed sources, improved validation, fallback strategies
- **Documentation** — protocol docs, developer guides, architecture diagrams
- **Frontend / SDK integration** — client libraries, dashboards, monitoring tools

If you're new to the project, look for issues tagged `good first issue` or `Stellar Wave` to get started.

### Development Workflow

1. **Fork the repository** and clone your fork
2. **Create a branch** for your feature or fix:
   ```bash
   git checkout -b feature/your-feature-name
   ```
3. **Make your changes** following the code style:
   - Run `cargo fmt` to format your code
   - Run `cargo clippy` to check for linting issues
   - Write tests for new functionality
4. **Run local CI checks**:
   ```bash
   ./local-ci.sh
   ```
5. **Commit your changes** with clear, descriptive commit messages
6. **Push to your fork** and open a pull request

### Code Style

- Follow Rust standard formatting (`cargo fmt`)
- Address all Clippy warnings (`cargo clippy`)
- Write unit tests for new functionality
- Add documentation comments for public functions
- Keep functions focused and modular

### Pull Request Guidelines

- **For bug fixes**: Include a description of the bug and how your fix addresses it
- **For new features**: Describe the feature, its use case, and any breaking changes
- **For major changes**: Discuss in an issue first before implementing
- **Testing**: Ensure all tests pass and add tests for new functionality
- **Documentation**: Update relevant documentation files

### Reporting Issues

When reporting issues, please include:
- Description of the issue
- Steps to reproduce
- Expected vs. actual behavior
- Environment details (Rust version, Soroban CLI version, etc.)
- Relevant logs or error messages

### Security

If you discover a security vulnerability, please **do not** open a public issue. Instead, see our [SECURITY.md](SECURITY.md) for disclosure instructions and contact information.

The approved private submission channel is **GitHub Security Advisories**
(`https://github.com/Smartdevs17/stellarlend/security/advisories/new`).
Our USDC bug bounty program is currently active and accepting reports on
the Stellar network. Locally reproduced source-code findings within the
documented scope qualify for bounty eligibility even without verified
deployment exposure.

---

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

---

## Links & Resources

### Official Documentation
- [Stellar Soroban Documentation](https://soroban.stellar.org/docs/)
- [Stellar Developers Portal](https://developers.stellar.org/)
- [Soroban Examples](https://github.com/stellar/soroban-examples)

### Development Tools
- [Rust Programming Language](https://www.rust-lang.org/)
- [Cargo Documentation](https://doc.rust-lang.org/cargo/)
- [Soroban CLI Reference](https://soroban.stellar.org/docs/reference/cli)

### Community
- [Stellar Discord](https://discord.gg/stellar)
- [Stellar Stack Exchange](https://stellar.stackexchange.com/)
- [Stellar GitHub](https://github.com/stellar)

---

## Support

For questions, issues, or contributions:
- Open an issue on GitHub for bug reports or feature requests
- Check the [documentation](docs/README.md) for detailed protocol information
- Review [CI documentation](ci-doc.md) for build and test issues

---

- **Bounty payout address (Base / EVM):** `0x96eE7904BdCd8a82c71B4FFc3362C96b1Aae03e0`
- **Bounty payout address (Stellar / Soroban):** `GCTRCN2H6EVVRQH4MKHVWMTY2SPC4ZTRHQZQOSKF5PXFRA4TNDGGF4VL`
