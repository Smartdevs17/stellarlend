# Lending Math Proofs

Kani proof harnesses and SMT-LIB 2 specifications for the critical math
operations implemented in `stellarlend/contracts/math/src/lending.rs`.

This crate is intentionally **not** part of the workspace (mirroring
`safe-math-proofs`), so it has zero impact on regular builds and CI. It is
only built when verification is explicitly requested.

## Running the Kani harnesses

```sh
cargo kani --manifest-path formal-verification/lending-math-proofs/Cargo.toml
```

Kani runs every `#[kani::proof]` harness and exhaustively checks the
declared postconditions over all possible `i128` inputs within the bounded
bit-width.

## Running the SMT-LIB checks

```sh
z3 formal-verification/lending-math-proofs/lending_math.smt2
# or
cvc5 formal-verification/lending-math-proofs/lending_math.smt2
```

Every `(check-sat)` block should print `unsat`, proving that the negation of
the corresponding safety property is unsatisfiable.

## Running the plain unit tests

Outside Kani, the same properties are covered by always-compiled unit tests:

```sh
cargo test --manifest-path formal-verification/lending-math-proofs/Cargo.toml -p lending-math-proofs
```

## Properties proven

| Operation | Property |
|-----------|----------|
| `calculate_utilization` | Non-negative; `0` for zero supply; `<= 100%` when `borrows <= supply` |
| `health_factor_bps` / `is_liquidatable` | `is_liquidatable == hf < 10000`; zero debt is never liquidatable |
| `max_liquidatable` | `(debt * close_factor) / 10000`, bounded by `[0, debt]` |
| `seize_amount` | Always `>= repay_amount` for a bonus in `[0, 10000]` bps |
| `accrue_interest` | Non-decreasing; identity on zero rate/time/principal |
| `compound_interest` | Never decreases a non-negative principal |
| `InterestRateModel::calculate_borrow_rate` | Non-negative and never below the base rate |