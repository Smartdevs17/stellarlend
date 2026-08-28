# Storage Layout & Slot Packing

This document is the storage layout reference for the StellarLend protocol
contract implementation (`contracts/hello-world`). It covers the packing scheme
introduced for pool configuration data (issue #722), migration for existing
deployments, and guidelines for future storage work.

## How storage is accessed

All contract state is read/written through `env.storage()`:

| Access tier | Use |
| --- | --- |
| `env.storage().instance()` | Per-contract-instance configuration that lives for the contract lifecycle |
| `env.storage().persistent()` | Pickled `#[contracttype]` values keyed by contract type — the main storage |
| `env.storage().temporary()` | Short-lived values with explicit TTL; not used for config |

## Packed pool configuration (issue #722)

The pool risk configuration used to be stored as one YAML-spread `RiskParams`
struct in a single slot:

```
RiskParamsConfig → RiskParams {
    min_collateral_ratio:  i128,   // 16 bytes
    liquidation_threshold: i128,   // 16 bytes
    close_factor:          i128,   // 16 bytes
    liquidation_incentive: i128,   // 16 bytes
    last_update:           u64,    //  8 bytes
}
// ≈ 72 bytes payload per read/write
```

All four bps fields are validated to the bound `0..=50_000` — far below the
`u16` range — so the whole record fits in a single `u128`.

### New packed layout

```
PackedRiskParamsConfig → u128
  bits  0..16   min_collateral_ratio   (u16 bps)
  bits 16..32   liquidation_threshold  (u16 bps)
  bits 32..48   close_factor           (u16 bps)
  bits 48..64   liquidation_incentive  (u16 bps)
  bits 64..128  last_update            (u64 timestamp)
// 16 bytes payload per read/write — ~4.5× smaller
```

Implemented in `risk_params.rs` as pure helpers:

- `pack_risk_params(&RiskParams) -> u128`
- `unpack_risk_params(u128) -> RiskParams`

The public API surface is unchanged: every reader still receives a
`RiskParams` via `get_risk_params(env)`. Only the on-chain representation
changed.

## Migration

Deployed contracts that were initialized before #722 carry the legacy slot
(`RiskParamsDataKey::RiskParamsConfig`). Migration is:

- **Lazy** — `get_risk_params` falls back to the legacy slot on first read
  after upgrade and migrates it to the packed slot.
- **Idempotent + explicit** — `risk_params::migrate_from_legacy(env)` returns
  `false` when migration is unnecessary; it is also exposed as the entrypoint
  `migrate_pool_config_packed`.

New installs (fresh `initialize_*`/`risk_initialize`) write only the packed
slot.

## Storage keys at a glance

| Key | Value | Tier | Notes |
| --- | --- | --- | --- |
| `RiskParamsDataKey::PackedRiskParamsConfig` | `u128` | persistent | packed pool config (#722) |
| `RiskParamsDataKey::RiskParamsConfig` | `RiskParams` | persistent | legacy slot, removed on migration |
| `GovernanceDataKey::Admin` | `Address` | instance | admin address |
| `GovernanceDataKey::Config` | `GovernanceConfig` | persistent | governance settings |
| `MevDataKey::Config` | `MevProtectionConfig` | persistent | MEV config (see `docs/mev-protection.md`) |
| `MevDataKey::SandwichLog` | `Vec<SandwichAttackRecord>` | persistent | bounded to 200 records |
| `MevDataKey::{Latest,Previous}Observation` | `OrderingObservation` | persistent | sandwich detection sequence |
| `TempDataKey::LendingIndexCache` | `LendingIndex` | temporary | tx-local index cache |

## Packing guidelines

1. **Pack small validated integers.** If a field's allowed range fits in `u16`
   (config bps, thresholds), do not store it as `i128`. Bit-pack with the
   layout documented directly next to the key.
2. **Keep the public view.** Expose the unpacked struct through readers; the
   packing is an on-chain representation detail.
3. **Migrate lazily or explicitly.** Never force a storage migration in a
   transaction user didn't ask for; `get_*` lazy fallback is the safest pattern.
4. **Bound log keys.** Never grow a `Vec` unboundedly in persistent storage —
   cap it (see `MAX_SANDWICH_LOG`).
5. **Document the bit layout.** Every packed key must have its bit map written
   as a comment at the enum variant and in this document.

## Gas savings & measurement

See `benchmarks/src/hello_world_benchmarks.rs` for `RiskParams` read/write
benchmarks and `docs/gas-reporting.md` for how they gate PRs. Expected effect
of #722: `get_risk_params`/`set_risk_params` write path payload drops from
~72 B to 16 B, reducing both CPU instructions and memory bytes for every risk
read that gates borrow/withdraw/liquidation.