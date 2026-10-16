# @stellarlend/sdk

Error-normalization helpers for frontend and backend consumers.

## What it gives you

- Stable numeric `ProtocolErrorCode` values.
- Human-readable messages for user-facing UI.
- Retry and handling hints for application logic.
- A normalization helper that converts unknown thrown values into a safe `ProtocolError`.

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
