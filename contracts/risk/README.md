# contracts/risk

`no_std` risk helpers shared by the pool contracts.

- `haircut` — collateral haircuts by volatility bucket (#1742).
- `cache` — `RiskWeightCache`, a fixed-capacity, allocation-free memo of
  risk-weight lookups scoped to one block height (#1774).

## RiskWeightCache semantics

| Situation | Behaviour | Counter |
| --- | --- | --- |
| First read of a key at a height | loader is called, value stored | `misses` |
| Repeat read at the same height | stored value returned | `hits` |
| Read at a different height (forward or rewound) | all entries dropped first | `resets` |
| Loader returns an error | error returned, nothing stored | `failures` |
| All slots taken | value computed and returned, not stored | `bypasses` |
| `invalidate()` | entries dropped, height kept | `invalidations` |

Lookups use open addressing, so the common case touches one slot rather than
scanning every registered asset. `CacheStats::hit_rate_bps()` reports the hit
rate for metrics export.

## Validation

```bash
cargo test --manifest-path contracts/risk/Cargo.toml
cargo test --manifest-path contracts/pool/Cargo.toml
```
