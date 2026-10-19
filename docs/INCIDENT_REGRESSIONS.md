# Incident Regression Index

Every past lending-contract incident has exactly one regression test in
[`stellar-lend/contracts/lending/tests/incident_regressions.rs`](../stellar-lend/contracts/lending/tests/incident_regressions.rs).
The suite runs in CI as part of the `lending-integration` suite in
`scripts/testing/run-contract-tests.sh` (workflow: `Contract Tests`).

| Incident | Date | Fix | Root cause | Regression test |
|---|---|---|---|---|
| #915 | 2026-09-23 | `739ed3cf` | `initialize` did not require the supplied admin to authorize, so anyone could front-run deployment and become admin | `incident_915_initialize_requires_admin_auth` |
| #1105 | 2026-09-24 | `f3246f67` | A second `initialize` on a live contract replaced the admin and reset accounting | `incident_1105_initialize_cannot_take_over_live_contract` |
| #923 | 2026-09-23 | `e4416dc7` | Configuration setters taking an `admin` argument must match it against the stored admin | `incident_923_config_setters_reject_non_admin_caller` |
| #935 | 2026-09-23 | `0b3b306f` | Deposit, withdraw and borrow settings, pause flags and the emergency limit could be changed without admin auth | `incident_935_config_setters_require_stored_admin_auth` |
| #1128 | 2026-09-25 | `b6d5d96f` | `deposit_collateral` credited collateral without the user's authorization, allowing fabricated positions | `incident_1128_collateral_deposit_requires_user_auth` |

## Adding an incident

1. Add a test named `incident_<issue>_<what_must_not_recur>` to
   `incident_regressions.rs`, with a doc comment naming the fix commit.
2. Add its name to `INCIDENT_TESTS` in the same file.
3. Add a row to the table above. The `index_lists_every_incident` test fails
   if a registered test is missing from this index.
