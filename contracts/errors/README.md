# `stellarlend-errors` (`contracts/errors`)

`no_std` crate defining the canonical StellarLend `ProtocolErrorCode` registry (`1000..=1999`) and deterministic mappings from per-contract `#[contracterror]` discriminants (`ContractDomain`) to unified protocol error codes.

## Usage

```rust
use stellarlend_errors::{ContractDomain, ProtocolErrorCode, map_contract_error};

let canonical = map_contract_error(ContractDomain::Oracle, 1);
assert_eq!(canonical, ProtocolErrorCode::OracleStale);
assert_eq!(canonical.as_u32(), 1020);
assert!(canonical.is_retryable());
```

## Testing

```bash
cargo test --manifest-path contracts/errors/Cargo.toml
```
