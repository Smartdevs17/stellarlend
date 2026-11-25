# `@stellarlend/error-codes` (`packages/error-codes`)

Unified protocol error registry, contract/backend error normalizer, and observability hooks shared across StellarLend `contracts/*`, `backend`, and `@stellarlend/sdk`.

## Features
- **Canonical Codes (`1000..=1999`)**: 1:1 synchronized with `contracts/errors` (`stellarlend-errors`) and `@stellarlend/sdk`.
- **Contract Domain Mapping**: Translates contract-local Soroban `#[contracterror]` discriminants (`Error(Contract, #N)`) by `ContractDomain` (`timelock`, `oracle`, `pause`, `flashloan`, `queue`, `keeper`, `migration`, etc.).
- **Backend Mapping**: Normalizes backend service error slugs (e.g. `KeeperReviewErrorCode`).
- **Observability Hooks**: Register an `ErrorObserver` via `setErrorObserver()` to emit structured logs, metrics, and trace attributes whenever errors are normalized.

## Testing

```bash
node --test test/error-codes.test.ts
```
