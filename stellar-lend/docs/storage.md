# Storage Layout

This document details the storage architecture of the StellarLend protocol.

## Packed Pool Configuration (#713)

To minimize storage costs and optimize I/O on the Soroban network, pool configuration settings are tightly packed into two primitive data types. By bit-packing configurations that were previously stored in multiple independent keys, we significantly reduce ledger access gas costs for every lending and borrowing operation.

### Data Structure

`PoolConfig` contains the following fields:
- `min_collateral_ratio_bps` (i128, stored in 16 bits)
- `liquidation_threshold_bps` (i128, stored in 16 bits)
- `reserve_factor_bps` (i128, stored in 16 bits)
- `close_factor_bps` (i128, stored in 16 bits)
- `liquidation_incentive_bps` (i128, stored in 16 bits)
- `last_update` (u64, stored in 40 bits)
- `flags` (u32, stored in 8 bits)

### Bitwise Packing Layout

The logical `PoolConfig` is packed into a `PackedConfig` composed of:
1. **`rate_word` (u128)**: Holds five 16-bit basis point values.
   - `Bits 0-15`: `min_collateral_ratio_bps`
   - `Bits 16-31`: `liquidation_threshold_bps`
   - `Bits 32-47`: `reserve_factor_bps`
   - `Bits 48-63`: `close_factor_bps`
   - `Bits 64-79`: `liquidation_incentive_bps`
   - `Bits 80-127`: Unused.
   
2. **`status_word` (u64)**: Holds timestamp and state flags.
   - `Bits 0-39`: `last_update` timestamp. This allows representing timestamps up to ~year 36,800.
   - `Bits 40-47`: `flags` for pool status (e.g., paused, borrowing_enabled, collateral_enabled).
   - `Bits 48-63`: Unused.

### Migration from Legacy Layout

The protocol automatically migrates from the legacy global `RiskParams` and per-asset `ReserveFactor` structures to the new per-pool `PackedConfig` the first time a pool's configuration is read via `get_pool_config()`.
