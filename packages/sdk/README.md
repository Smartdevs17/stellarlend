# @stellarlend/sdk

Typed TypeScript client and error-normalization helpers for frontend and backend
consumers.

## What it gives you

- Stable numeric `ProtocolErrorCode` values.
- Human-readable messages for user-facing UI.
- Retry and handling hints for application logic.
- A normalization helper that converts unknown thrown values into a safe `ProtocolError`.
- Exponential backoff for retryable protocol failures, configurable per client.
- Async iterators that auto-paginate list endpoints.
- Typed transaction builders that validate parameters before anything is signed.

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

## Pagination

`paginate` turns any page fetcher into an async iterator. Pages are fetched lazily, so
breaking out of the loop stops further requests:

```ts
import { paginate, collect } from '@stellarlend/sdk';

// Cursor-based endpoint: return the page's items and the next cursor (null when done).
const events = paginate(async ({ cursor, limit }) => {
  const page = await api.listEvents({ cursor, limit });
  return { items: page.events, nextCursor: page.next };
}, { pageSize: 100, maxItems: 1_000 });

for await (const event of events) handle(event);
```

| Helper | Use |
|---|---|
| `paginate(fetchPage, options)` | Iterate items |
| `paginatePages(fetchPage, options)` | Iterate whole pages (e.g. to show page boundaries) |
| `offsetFetcher((offset, limit) => …)` | Adapt an offset/limit endpoint; a short page ends iteration |
| `collect(iterable)` | Drain into an array (pair with `maxItems` on large sources) |
| `client.paginate(method, args, options)` | Read-only contract entrypoints shaped `(…args, offset: u32, limit: u32)` |

Options: `pageSize` (default `DEFAULT_PAGE_SIZE` = 50), `startCursor` to resume,
`maxItems` to cap the total, and `maxPages` as a guard. A fetcher that returns the same
cursor twice raises an error instead of looping forever.

```ts
for await (const recipient of client.paginate<string>('get_recipients_page')) {
  console.log(recipient);
}
```

Plain numbers in contract arguments are `u32` values; amounts stay `bigint` (`i128`).

## Transaction builders

`client.tx` builds typed, validated invocations without sending them. Each builder
returns a `TypedInvocation<R>` that carries the entrypoint's result type, and
`client.execute` runs it with the client's retry policy:

```ts
const steps = [
  client.tx.deposit({ user, asset: XLM, amount: 150n * UNIT }),           // TypedInvocation<bigint>
  client.tx.borrow({ user, asset: USDC, amount: 100n * UNIT,
                     collateralAsset: XLM, collateralAmount: 150n * UNIT }), // TypedInvocation<void>
];
showForReview(steps);                       // plain objects: method, args, source, readonly
const balance: bigint = await client.execute(steps[0]);
```

Builders throw `TypeError`/`RangeError` on missing addresses or non-positive amounts,
before anything reaches a wallet. Use `new LendingTransactions({ contractId, source })`
to build without a client, `tx.withSource(address)` to build for another account, and
`tx.custom<R>(method, args, readonly)` for entrypoints without a dedicated builder.
The client's own `deposit`, `borrow` and other methods are thin wrappers over these
builders.

## Examples

See [`examples/`](examples/README.md) for type-checked, tested examples: a Soroban RPC
adapter, the deposit/borrow/repay/withdraw lifecycle, error handling with retries, wallet
connection, and reading the utilization circuit breaker. The repository-wide
[examples workspace](../../examples/README.md) has runnable apps for pagination and
transaction builders, plus an index of every other example.

## Handling strategy

1. Always branch on `code` first.
2. Use `message` for display only.
3. Treat `Unknown` as a safe fallback and log the raw payload.
4. For cross-contract failures, preserve the downstream code when the caller can decode it.
