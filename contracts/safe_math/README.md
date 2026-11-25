# stellarlend-safe-math

`no_std` overflow-safe `i128` arithmetic shared by StellarLend contracts.

| Need | Use |
|------|-----|
| Overflow must fail the transaction | `checked_mul_div`, `apply_bps` |
| Overflow should clamp (caps, display values) | `saturating_mul_div`, `saturating_apply_bps`, `saturating_add/sub/mul` |
| Remaining capacity, never negative | `saturating_sub_floor` |

Division by zero and negative bps/amounts always return an error, even in the
saturating variants. Test with `cargo test` from this directory.
