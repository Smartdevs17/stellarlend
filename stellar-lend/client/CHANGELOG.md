# Changelog

All notable changes to `stellarlend-client` are documented here. The crate
follows [Semantic Versioning](https://semver.org/); releases are tagged
`client-v<version>` and published by `.github/workflows/sdk-release.yml`.

## 0.1.0 – Unreleased

- Horizon and Soroban RPC clients with transaction submission and monitoring.
- Retry defaults with exponential backoff (3 retries, 100ms initial delay,
  x2 multiplier, 5s cap), configurable with `with_max_retries` and
  `with_retry_config`.
- Examples: `simple_transaction` and `monitor_transaction`.
