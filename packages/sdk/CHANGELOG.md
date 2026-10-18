# Changelog

All notable changes to `@stellarlend/sdk` are documented here. The package
follows [Semantic Versioning](https://semver.org/); releases are tagged
`sdk-v<version>` and published by `.github/workflows/sdk-release.yml`.

## 0.1.0 – Unreleased

- Typed `StellarLendClient` for deposit, withdraw, borrow, repay, liquidate and
  position reads, over a pluggable `ContractAdapter`.
- `ProtocolError` normalization with user-facing messages and retry hints.
- Retry defaults with exponential backoff for read-only calls (3 retries,
  100ms initial delay, x2 multiplier, 5s cap), configurable through the
  `retry` client option; writes retry only with `retryWrites: true`.
- Runnable examples: borrow lifecycle, error handling, Soroban RPC adapter,
  utilization breaker and wallet connection.
