# Read Cache and Prefetching

Read-heavy data (protocol stats, pool state, user positions, gas analytics, simulations) is served through one read cache with background prefetching of the entries that are read most. The store is Redis when `REDIS_ENABLED=true` and an in-memory map otherwise, so every environment gets the same behaviour.

## Layers

| Layer | Where | What it does |
|-------|-------|--------------|
| Read-through service | `api/src/services/readCache.service.ts` | `getOrLoad(kind, id, ttlMs, loader)`: cache lookup, single in-flight load per key, store, access reporting to prefetch. `invalidate(kind, id?)` and `invalidateAll()`. Per-kind hit, miss, load, error, store and invalidation counters. |
| GET response cache | `api/src/middleware/readCache.middleware.ts` | `cacheReadResponse({ ttlMs })` stores the JSON body of 2xx GET responses keyed by path plus sorted query. Answers repeats without running the handler and sets `X-Cache: HIT | MISS | BYPASS | SKIP`. Replays the handler's `Cache-Control`. |
| Prefetch | `api/src/services/prefetch.service.ts`, `api/src/jobs/prefetch.job.ts` | Counts reads per key, decays the counts every window, and refreshes hot keys in the background before they expire. Protocol stats are pinned so they are always warm. |
| Admin endpoints | `api/src/routes/cache.routes.ts` | Metrics, invalidation and manual prefetch runs. |

## Which reads are cached

Service level (read-through, tracked for prefetch):

- `StellarService.getProtocolStats` (`protocol:stats`, `PROTOCOL_STATS_TTL_MS`)
- `StellarService.getUserPosition` (`position:<address>`, `POSITION_CACHE_TTL_MS`)
- Transaction simulations (`simulation:tx:<sha256>`, `SIMULATION_CACHE_TTL_MS`)

Response level (`X-Cache` header present):

- `GET /api/protocol/stats` (TTL `PROTOCOL_STATS_TTL_MS`)
- `GET /api/gas/compare`, `/historical/:operation`, `/chart/:operation`, `/analytics`, `/forecast/:operation` (TTL `READ_CACHE_DEFAULT_TTL_MS`)

To cache another GET route add the middleware in front of the handler:

```ts
import { cacheReadResponse } from '../middleware/readCache.middleware';

router.get('/summary', cacheReadResponse({ ttlMs: 30000 }), controller.getSummary);
```

To cache a service lookup and make it eligible for prefetching:

```ts
import { readCacheService } from './readCache.service';

return readCacheService.getOrLoad('pool', poolKey, config.cache.poolTtlMs, () => this.loadPoolState(poolKey));
```

Loaders that return `null` or `undefined` are not cached. Responses other than 2xx are not cached. Handlers that answer with `res.send(string)` are not cached; the middleware only intercepts JSON bodies.

## Invalidation

- A successful `POST /api/lending/submit` drops the `position`, `pool`, `protocol`, `simulation` and `http` kinds.
- `POST /api/cache/invalidate` (operator role) drops everything readable with an empty body, one kind with `{ "kind": "position" }`, or one entry with `{ "kind": "position", "id": "G..." }`. The `nonce` kind is never touched.
- Clients bypass the response cache for one request with `Cache-Control: no-cache`.

## Prefetching

Every read through `getOrLoad` counts one access for its key and remembers the loader. A key whose count reaches `PREFETCH_HOT_THRESHOLD` within a window is hot. Every `PREFETCH_INTERVAL_MS` the scheduler takes up to `PREFETCH_MAX_KEYS_PER_RUN` hot keys (pinned first, then by count), skips those loaded less than half a TTL ago, runs their loaders and rewrites the cache entry. Counts are halved every `PREFETCH_WINDOW_MS`; keys that reach zero are forgotten, and the coldest key is evicted when `PREFETCH_MAX_TRACKED_KEYS` is reached.

The scheduler starts with the server (`startPrefetchScheduler()` in `api/src/index.ts`) and is disabled with `PREFETCH_ENABLED=false`. Loader failures are counted and logged; they never abort a run.

## Endpoints

| Method | Path | Role | Response |
|--------|------|------|----------|
| GET | `/api/cache/stats` (`/api/v1/system/cache/stats`) | public | `readCache` counters per kind with total hit rate, `store` counters, `prefetch` state |
| POST | `/api/cache/invalidate` | operator | `{ success, scope, kind?, id? }` |
| GET | `/api/cache/prefetch` | public | scheduler state, hot keys (count, pinned, refreshes, failures) |
| POST | `/api/cache/prefetch/run` | operator | `{ success, refreshed, failed, skipped, durationMs }` |

Example:

```bash
curl http://localhost:3000/api/cache/stats
curl -X POST http://localhost:3000/api/cache/invalidate -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{"kind":"position","id":"G..."}'
```

## Configuration

| Variable | Default | Meaning |
|----------|---------|---------|
| `READ_CACHE_ENABLED` | `true` | `false` bypasses both cache layers |
| `READ_CACHE_DEFAULT_TTL_MS` | `15000` | TTL for cached GET responses without a route-specific TTL |
| `READ_CACHE_MAX_KEY_LENGTH` | `512` | Longest path plus query eligible for response caching |
| `PREFETCH_ENABLED` | `true` | Background refresh on or off |
| `PREFETCH_INTERVAL_MS` | `10000` | Scheduler period |
| `PREFETCH_WINDOW_MS` | `60000` | Access-count decay window |
| `PREFETCH_HOT_THRESHOLD` | `5` | Accesses per window that make a key hot |
| `PREFETCH_MAX_TRACKED_KEYS` | `1000` | Tracked-key cap |
| `PREFETCH_MAX_KEYS_PER_RUN` | `50` | Refreshes per scheduler run |

## Tests

`readCache.service.test.ts`, `readCache.middleware.test.ts`, `prefetch.service.test.ts`, `cache.routes.test.ts`.
