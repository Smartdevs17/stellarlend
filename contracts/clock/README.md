# StellarLend contract clock

`stellarlend-clock` provides one source of contract time: the active Soroban
ledger timestamp in Unix seconds. Contract crates should use
`stellarlend_clock::now_seconds(&env)` or the `Clock` trait instead of reading
the ledger directly. This keeps ledger time distinct from backend wall-clock
milliseconds and makes clock-dependent logic straightforward to test.

Run its unit tests with:

```bash
cargo test --manifest-path contracts/clock/Cargo.toml
```
