# @stellarlend/sdk

Typed TypeScript client and error-normalization helpers for frontend and backend
consumers.

## What it gives you

- Stable numeric `ProtocolErrorCode` values.
- Human-readable messages for user-facing UI.
- Retry and handling hints for application logic.
- A normalization helper that converts unknown thrown values into a safe `ProtocolError`.
- Exponential backoff for retryable protocol failures, configurable per client.

The package is versioned independently and published as `@stellarlend/sdk`.

Install with `npm install @stellarlend/sdk`. Releases follow semver, are tagged
`sdk-v<version>`, and are listed in [CHANGELOG.md](CHANGELOG.md).

## Retry defaults

Read-only client calls retry retryable protocol errors up to three times,
waiting 100ms, 200ms, and 400ms between attempts (each delay capped at 5
seconds; see `DEFAULT_RETRY_OPTIONS`). Configure the policy
when constructing a client:

```ts
const client = new StellarLendClient({
  contractId,
  adapter,
  retry: {
    maxRetries: 5,
    initialDelayMs: 250,
    maxDelayMs: 2_000,
    multiplier: 2,
  },
});
```

Non-retryable protocol errors are thrown immediately, and `maxRetries: 0`
disables retrying. State-changing calls are not retried by default, because a
write that failed ambiguously may already have applied. Set
`retryWrites: true` only when your adapter submits idempotently.

## Usage

```ts
import { normalizeProtocolError } from '@stellarlend/sdk';

try {
  // contract call
} catch (error) {
  const normalized = normalizeProtocolError(error);
  console.error(normalized.code, normalized.message, normalized.handlingStrategy);
}
```

## Examples

See [`examples/`](examples/README.md) for type-checked, tested examples: a Soroban RPC
adapter, the deposit/borrow/repay/withdraw lifecycle, error handling with retries, wallet
connection, and reading the utilization circuit breaker.

## Handling strategy

1. Always branch on `code` first.
2. Use `message` for display only.
3. Treat `Unknown` as a safe fallback and log the raw payload.
4. For cross-contract failures, preserve the downstream code when the caller can decode it.
