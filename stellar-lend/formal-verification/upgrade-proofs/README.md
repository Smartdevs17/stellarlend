# upgrade-proofs

Formal verification of the StellarLend upgrade mechanism safety:
unauthorized state change prevention, timelock enforcement, and
fund safety during contract upgrades.

This crate is **not** part of the main Cargo workspace (see the root
`Cargo.toml` `exclude` list), matching the existing
`formal-verification/safe-math-proofs` pattern, so it doesn't affect
normal `cargo build`/`cargo test` runs of the contract workspace.

## Running

Bounded property tests (fast, run in normal CI, no extra tooling):

```sh
cargo test --manifest-path formal-verification/upgrade-proofs/Cargo.toml
```

Exhaustive bounded model checking with [Kani](https://model-checking.github.io/kani/):

```sh
cargo install --locked kani-verifier
cargo kani --manifest-path formal-verification/upgrade-proofs/Cargo.toml
```

SMT-LIB spec directly with Z3:

```sh
z3 formal-verification/upgrade-proofs/upgrade_spec.smt2
```

## Properties verified

### Oracle Hub Upgrade (`oracle-hub/src/upgrade.rs`)

1. **Unauthorized staging prevention**: only governance can stage
   an upgrade; unauthorized callers cannot set the proposed WASM hash.
2. **Multisig threshold enforcement**: an upgrade cannot be
   executed unless the number of approvals meets or exceeds the
   installed threshold.
3. **Timelock enforcement**: an upgrade cannot be executed before
   the 48-hour timelock has elapsed (when threshold > 1).
4. **Version monotonicity**: each successful upgrade bumps the
   stored version number by exactly 1.
5. **No fund loss**: the upgrade mechanism only swaps contract code
   via `env.deployer().update_current_contract_wasm`; it never
   transfers or drains funds.
6. **Staged candidate cleared on execution**: the proposed WASM
   hash is removed from storage after a successful upgrade.

### Migration Hub Upgrade (`migration-hub/src/upgrade.rs`)

7. **Proposal authorization**: only authorized approvers can
   approve a migration proposal.
8. **Execution authorization**: a migration proposal can only be
   executed after sufficient approvals have been collected.
9. **Rollback safety**: emergency rollback requires admin
   authorization and only applies to completed migrations.

See `src/lib.rs` for the full description of properties verified,
and `upgrade_spec.smt2` for the corresponding SMT-LIB 2 encoding.
