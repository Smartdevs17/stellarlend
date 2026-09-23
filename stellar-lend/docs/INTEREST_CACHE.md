# Incremental Interest Cache

The lending contracts maintain cumulative borrow and supply indices instead of
recomputing each position's complete interest history on every interaction.

## Update model

For each newly elapsed segment:

```text
delta = index * rate_bps * elapsed_seconds / (10_000 * seconds_per_year)
next_index = index + delta
```

The cache records the cumulative index, last update timestamp, last ledger, and
the rate used by the open segment. A rate change first closes elapsed time at
the previous rate, then stores the new rate for subsequent time. Calls in the
same ledger reuse the persisted result and avoid another write.

Position interest is calculated in constant time:

```text
interest = principal * (current_index - entry_index) / entry_index
```

## Read and invalidation paths

- `update_lending_index` and `update_interest_cache` increment and persist the
  cache on mutation paths.
- `preview_interest_index` computes the current value without a storage write
  for read-only calls.
- The core contract's temporary cache is ledger-scoped. Soroban temporary
  entries can outlive a transaction, so entries are accepted only when their
  timestamp matches the current ledger timestamp.
- Rate-model changes should update/invalidate the cache at the same point as
  the configuration mutation, ensuring the prior segment is never repriced.

## Performance coverage

The benchmark suite includes full-recompute, incremental-update, and
same-ledger cache scenarios. Packed pool configuration read/write benchmarks
cover the storage path that supplies rate and reserve parameters.
