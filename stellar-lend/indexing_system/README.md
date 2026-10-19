# Blockchain indexing system operations

The indexer applies SQL migrations on initialization. To run the backfill or
reorg operations, initialize the system, register the contract ABI, then call
the service methods:

```rust
indexer.register_contract(address, abi_json, start_block).await?;
let result = indexer.backfill(address, from_block, to_block).await?;
```

`backfill` processes the inclusive block range in configured batch sizes and
stores a checkpoint per contract and exact requested range. If a batch fails,
call `backfill` again with the same contract, `from_block`, and `to_block`; it
resumes after the last committed batch. Event writes are idempotent by
transaction hash and log index. Use a new range key to intentionally rerun a
different range.

When a chain reorganization is detected, call
`indexer.handle_reorg(first_replaced_block).await?`. Event deletion and
contract checkpoint rewind run in one database transaction. Repeating the
operation is safe; the indexer will fetch the affected blocks again on its
next poll.

Run the unit tests with `cargo test --manifest-path Cargo.toml` from this
directory. Database migration execution should also be verified against the
PostgreSQL version used in deployment before applying it to production.
