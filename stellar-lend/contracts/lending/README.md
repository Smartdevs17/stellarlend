# StellarLend Lending Contract

A secure, efficient lending protocol built on Soroban that allows users to borrow assets against collateral.

## Features

- **Collateralized Borrowing**: Borrow assets by providing collateral with a minimum 150% ratio
- **Multi-Collateral Positions**: Up to 8 collateral assets per position; the health factor sums all of them
- **Liquidation**: Liquidators repay debt of unhealthy positions and seize any collateral asset, with a per-asset incentive
- **Configurable Interest Rates**: Utilization-based kinked rate model with admin-set parameters
- **Debt Ceiling**: Protocol-level debt limits for risk management
- **Pause Mechanism**: Granular emergency pause functionality for specific operations (Deposit, Borrow, Repay, Withdraw, Liquidation)
- **Emergency Lifecycle**: `Normal -> Shutdown -> Recovery -> Normal` flow with guardian-triggered shutdown and admin-controlled recovery
- **Admin Control**: Secure protocol management with a dedicated admin role
- **Overflow Protection**: Comprehensive checks against arithmetic overflow
- **Event Emission**: Track all borrow and pause operations via events

## Building

```bash
cargo build --target wasm32-unknown-unknown --release
```

## Testing

```bash
cargo test
```

## Documentation

See [borrow.md](./borrow.md), [liquidation.md](./liquidation.md), [interest_rate_models.md](./interest_rate_models.md), [pause.md](./pause.md), and [emergency_shutdown.md](./emergency_shutdown.md) for comprehensive documentation including:

- Function signatures and parameters
- Error types and handling
- Security assumptions
- Usage examples
- Best practices

## Contract Interface

### Main Functions

- `borrow()` - Borrow assets against collateral
- `get_user_debt()` - Query user's debt position
- `get_user_collateral()` - Query user's collateral position (largest asset)
- `get_user_collaterals()` - Query every collateral asset in the user's position
- `deposit_collateral()` - Add collateral of any asset to a borrow position
- `liquidate()` - Liquidate an unhealthy position, seizing a chosen collateral asset
- `get_borrow_rate_bps()` / `get_supply_rate_bps()` / `get_utilization_bps()` - Current rates

### Admin Functions

- `initialize()` - Set protocol admin, debt ceiling, and minimum borrow amount
- `set_pause()` - Granular pause for specific operations
- `set_guardian()` - Configure emergency guardian
- `emergency_shutdown()` - Trigger hard emergency shutdown (admin or guardian)
- `start_recovery()` - Enter controlled user unwind mode (admin only)
- `complete_recovery()` - Return protocol to normal operation (admin only)
- `get_admin()` - Returns the current protocol admin
- `set_liquidation_incentive_bps()` / `set_asset_liq_incentive_bps()` - Global and per-asset liquidation incentive (1%–25%)
- `set_interest_rate_config()` - Update interest rate model parameters

## Security

- Minimum 150% collateral ratio enforced
- All arithmetic operations use checked methods
- Authorization required for all user operations
- Comprehensive test coverage including edge cases

## License

See repository root for license information.
