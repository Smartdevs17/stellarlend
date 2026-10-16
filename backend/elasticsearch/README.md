# Event indexing and storage

How StellarLend indexes contract events, answers queries, archives old events, and replays them for debugging (issue #685).

```
Soroban RPC getEvents ──► EventIndexer (api/src/services/eventIndex)
                            ├─ normalize + schema upcast   (schema.ts)
                            ├─ hot store, in-memory        (store.ts)   ◄── GET /api/events
                            ├─ Elasticsearch sink          (elasticsearch.ts, optional)
                            └─ cold archive, gzip NDJSON   (archive.ts) ◄── includeArchived / replay
```

| Tier | What it holds | Used for |
|---|---|---|
| Hot store (API memory) | Events newer than `EVENT_HOT_RETENTION_DAYS` | Query API, analytics, dashboards |
| Elasticsearch (this folder) | Every ingested event, managed by ILM | Kibana, ad-hoc search, long-range aggregations |
| Cold archive (`EVENT_ARCHIVE_DIR`) | Day segments of evicted events, checksummed | Historical queries, replay, re-indexing; the long-term record |

## Ingestion

`EventIndexer.sync()` pages through `getEvents` for `CONTRACT_ID`, starting at `EVENT_INDEXER_START_LEDGER` (or about one day before the latest ledger) and then resuming from the RPC cursor. Events from failed contract calls are skipped. Events are deduplicated by RPC event id, so re-syncing a range is safe.

Each event is normalized to an `IndexedEvent`:

- **`type`**: the event's first topic, minus `_event`. For example, `DepositEvent` becomes `deposit`, `WithdrawalEvent` becomes `withdrawal`, and structured envelopes become `proto_evt`.
- **`actor` / `counterparty`**: the address topics. For example, the user, or the liquidator and borrower.
- **`module` / `action`**: set only for `proto_evt` envelopes.
- **`accounts`**: every address in the topics or payload. This is what the `account` filter matches.
- **Numbers**: `i128` and `u64` values are stored as decimal strings, so no precision is lost.

## Schema versioning

- `proto_evt` envelopes carry `schema_version`. It mirrors `EVENT_SCHEMA_VERSION` in `stellar-lend/contracts/hello-world/src/events.rs`.
- Typed events are treated as version 1.
- When the contract bumps the version, add an upcaster for the old version to `DEFAULT_UPCASTERS` in `schema.ts`. Upcasters chain, and older payloads are stored with `schemaStatus: "upcast"`.
- Events with a newer version than the indexer knows about are kept as they are, with `schemaStatus: "unknown"`. Nothing is dropped during an upgrade. `/api/events/analytics` reports `bySchemaStatus`, so you can see when this happens.

## Query API

Endpoints are mounted at `/api/events`:

| Endpoint | Description |
|---|---|
| `GET /` | Filters: `type`, `account` (alias `address`), `contract`, `module`, `action`, `from`/`to` (epoch ms or ISO), `fromLedger`/`toLedger`, `order`, `limit` (≤1000), `cursor`, `includeArchived`. Returns `{ events, nextCursor, matched, tookMs, source }`. |
| `GET /:id` | Returns one event. |
| `GET /types`, `GET /stats` | Known and indexed event types, with counts. |
| `GET /analytics` | Counts by type, module, and schema status; an hourly or daily time series; top accounts; volume per asset; query latency (p50, p95, max). |
| `GET /indexer/status` | Cursor, lag in ledgers, last error, archive summary, sink failures. |
| `POST /indexer/sync` | Operator only. Pulls new events now. |
| `GET /archive`, `POST /archive` | Reads the archive manifest. The POST (operator only) archives events older than `olderThanDays`. |
| `POST /replay` | Operator only. Rebuilds account state from events (see below). |

`GET /` stays well under 100 ms. The index keeps per-type and per-account lists in ledger order and binary-searches time, ledger, and cursor bounds. `api/src/__tests__/eventIndex.performance.test.ts` checks the p95 latency of typical filters over 200k events against a 100 ms budget. Locally, most filters take about 1–6 ms and a full-scan filter about 20 ms.

## Archival

`POST /archive` (or `EventIndexer.archive()`) takes events older than the retention window out of the hot store and writes them as one gzip NDJSON segment per UTC day to `events/<day>/segment-<n>.ndjson.gz`. `manifest.json` records each segment's ledger range, time range, count, and SHA-256 checksum. Reads verify the checksum.

- **Re-archiving a day** merges the new events into a new segment generation. The manifest is written last, so a crash never leaves it pointing at a partly written segment.
- **Failed writes** put the evicted events back into the hot store, so no data is lost.
- **Other storage backends**: `ColdStorage` has only `put` and `get`. Swapping `FileSystemColdStorage` for an S3 or GCS adapter doesn't require changes to the archiver.

## Replay for debugging

`POST /api/events/replay` with any of the query filters (and `includeArchived`, which defaults to true) re-applies events in ledger order. It returns:

- `finalState`: collateral and debt per account and asset.
- `transitions`: every before/after balance change, capped by `maxTransitions`.
- `anomalies`: balances that went negative. This means events are missing from the window or index, or the contract has an accounting bug.

Replay uses typed events by default. Pass `"source": "structured"` to replay the `proto_evt` layer instead and cross-check the two.

## Elasticsearch setup

```bash
docker compose -f backend/elasticsearch/docker-compose.yml up -d   # local only; security disabled
ELASTICSEARCH_URL=http://localhost:9200 ./backend/elasticsearch/setup.sh
```

`setup.sh` is idempotent. It installs:

- **`ilm-policy.json`**: rolls indices over after 7 days or at 30 GB per primary shard. Indices move to warm (force-merge and shrink) at 30 days and to cold (read-only) at 90 days, and are deleted from the search tier at 365 days.
- **`index-template.json`**: strict mappings for `stellarlend-events-*`. Exact `i128` amounts are stored as a keyword. `amount_numeric` holds a lossy numeric copy for aggregations. The payload is a `flattened` field, so new event fields don't cause a mapping explosion.
- **The bootstrap index**: `stellarlend-events-000001`, behind the `stellarlend-events` write alias.

The API writes to the alias with bulk `create` actions, using the RPC event id as `_id`. This makes re-ingestion idempotent (409 responses are ignored). A sink failure is counted in `/indexer/status` but never blocks ingestion.

## Configuration

| Variable | Default |
|---|---|
| `EVENT_INDEXER_ENABLED` | `false`. When `true`, the indexer polls on API startup. |
| `EVENT_INDEXER_POLL_MS` | `10000` |
| `EVENT_INDEXER_START_LEDGER` | latest ledger minus about one day |
| `EVENT_HOT_RETENTION_DAYS` | `30` |
| `EVENT_ARCHIVE_DIR` | `./data/event-archive` |
| `ELASTICSEARCH_URL` | unset, which disables the sink |
| `ELASTICSEARCH_API_KEY` | unset |
| `ELASTICSEARCH_EVENT_INDEX` | `stellarlend-events` |
