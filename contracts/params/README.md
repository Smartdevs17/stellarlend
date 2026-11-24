# contracts/params (issue #1760)

`ParameterRegistry` replaces scattered `Map<address, (i128, i128, ...)>` values
with one typed, validated value object. It is deliberately storage-agnostic:
contracts persist the whole registry under a single key and wrap mutations in
their own governance checks, which keeps the invariants in one place.

## Invariants

- `0 < collateral_factor_bps <= liquidation_threshold_bps <= 10_000`
- `reserve_factor_bps <= 5_000`
- `borrow_cap == 0` means unlimited; negative caps are rejected
- updates are atomic — an invalid replacement leaves the old entry intact
- frozen (disabled) assets contribute no borrowing power

## Usage

```rust
use stellarlend_params::registry::{AssetParameters, ParameterRegistry};

let mut registry = ParameterRegistry::new();
registry.register(AssetParameters::new(1, 7_500, 8_300, 1_000, 0, 1)?)?;
assert_eq!(registry.borrow_capacity(1, 20_000)?, 15_000);
```

`contracts/pool/src/config.rs` embeds this registry in `PoolConfig`, so a pool
has one typed configuration object instead of per-asset globals.

## Validation

```bash
cargo test --manifest-path contracts/params/Cargo.toml
```
