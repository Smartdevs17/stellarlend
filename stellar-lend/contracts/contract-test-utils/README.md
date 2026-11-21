# contract-test-utils

Shared Rust test fixtures for StellarLend Soroban contracts.

This crate is the contract-facing package for shared fixtures, environment
helpers, mock contracts, seeded data, and assertions. It currently re-exports
the unified `test-utils` crate so existing suites can migrate incrementally.

## Usage

```toml
[dev-dependencies]
contract-test-utils = { path = "../contract-test-utils" }
```

```rust
#[cfg(test)]
mod tests {
    use contract_test_utils::{ProtocolFixture, TestEnv};

    #[test]
    fn example() {
        let env = TestEnv::new();
        let fixture = ProtocolFixture::builder().users(2).tokens(1).build();
        assert_eq!(fixture.users.len(), 2);
        let _ = env;
    }
}
```

## Migration

- Existing `test-utils` dependency remains supported.
- New or touched contract tests should prefer `contract-test-utils`.
- Shared fixtures belong in `test-utils` modules; this crate exposes them with
  a stable contract-test package name.
