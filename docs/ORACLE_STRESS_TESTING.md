# Oracle Stress Testing

Reference for the oracle stress test suite introduced in issue #691.

Before this suite, oracle testing covered normal operation: a provider answers,
the price validates, the contract stores it. That tells you the happy path
works. It does not tell you what happens when every provider is down, when one
of them is lying, when a 60% crash prints in a single tick, or when a breaker
opens during a liquidation cascade — which is when an oracle actually decides
whether a lending protocol survives.

## The invariant

Everything here exists to protect one property:

> The protocol never acts on a price that failed a guard. It fails closed —
> returning an error — instead of returning a price it could not certify.

The asymmetry is the point. A blocked operation is an inconvenience; a wrong
price liquidates solvent positions irreversibly. Every scenario below asserts
some version of "returns an error, not a number".

The second-order property, equally important, is that **failing closed is always
temporary**. A guard that suppresses a price during a crash but never lets the
feed recover leaves the protocol permanently blind — which is its own
catastrophe. Several scenarios exist purely to prove recovery happens without
manual intervention.

## Layout

The oracle spans three layers, and a failure in any one of them reaches users.
Each layer has its own suite, because each fails differently.

| Layer | Location | Run with |
| --- | --- | --- |
| Off-chain aggregator | `oracle/tests/stress/` | `cd oracle && npm run test:stress` |
| Oracle → contract → API | `tests/e2e/oracle-stress.e2e.test.ts` | `cd tests/e2e && npx jest oracle-stress` |
| On-chain guards | `stellar-lend/contracts/hello-world/src/tests/oracle_stress_test.rs` | `cd stellar-lend && cargo test -p hello-world oracle_stress` |

CI runs all three as separate jobs in
[`.github/workflows/oracle-stress-tests.yml`](../.github/workflows/oracle-stress-tests.yml),
on every oracle change and nightly.

> **Note on the contract suite.** The `hello-world` crate does not currently
> compile — it references roughly 35 functions that no longer exist across
> `circuit_breaker`, `reentrancy`, `borrow`, `deposit`, `liquidate`, `repay`,
> `withdraw`, `analytics`, `health`, `risk_management`, `recovery`,
> `interest_rate`, `multi_collateral`, `risk_params` and `monitor`. That
> breakage predates this suite and is unrelated to the oracle. The scenarios are
> written and wired into `src/tests/mod.rs`, and its CI job is marked
> `continue-on-error` so it reports without blocking until the crate builds
> again. Drop that flag once it does.

## Scenario coverage

44 scenarios at the service layer, 18 end to end, 28 on chain.

| Category | What it establishes |
| --- | --- |
| **Failure scenarios** | Total outage, 70% intermittent failure, malformed payloads, day-old timestamps, a byzantine provider reporting a plausible-but-wrong price, and quorum starvation. In every case the aggregate is either correct or absent. |
| **Price spike / crash** | Instantaneous ±60–150% moves are suppressed; gradual moves inside the guard bands are tracked tick for tick; a sustained crash suppresses mid-fall and then republishes once the market settles; a lone manipulated quote is absorbed by its peers. |
| **Latency** | Distribution under concurrent load, the cost of aggregation mode (gated by the slowest provider) versus failover mode (which skips it), a minority latency spike that must not drag the median, and cache hits that issue zero provider calls. |
| **Circuit breaker** | Opens exactly at threshold; an interleaved success resets the count; a failed half-open probe re-opens immediately; breakers are isolated per provider; and recovery is automatic after backoff. |
| **Multi-oracle redundancy** | Progressive degradation from five oracles to one, clean stop when quorum is lost, observable source-count reduction, failover chains, non-sticky primary preference, and tolerance of a byzantine minority. |
| **Oracle upgrade** | Provider swaps, weight changes, aggregation-mode switches, threshold tightening, quorum raises, rolling restarts, and exact rollback. |

## Determinism

Stress tests that flake get muted, and a muted suite protects nothing. Three
choices keep these reproducible:

**Seeded randomness.** Every provider that fails probabilistically draws from a
seeded `mulberry32` generator, so a failing run reproduces exactly.

**Simulated time where guards are time-based.** The TWAP window ages samples by
wall clock and the price cache expires by wall clock. Run against the real
clock, an entire simulated price path executes inside a millisecond or two — the
cache never expires, the TWAP window never advances, and the suite ends up
measuring its own execution speed. Scenarios that exercise those guards use
fake timers and advance the clock a simulated minute per tick.

**Relationships, not absolute budgets, for latency.** The latency scenarios run
against the real clock, since elapsed time is the subject. They assert things
like "p99 stays within 10× p50" and "a cache hit issues zero provider calls"
rather than "this completes in under 50ms", which would fail on a loaded runner.

### A cache subtlety worth knowing

`createPriceCache(0)` does **not** disable caching. A TTL of zero sets
`expiresAt = Date.now()`, and the cache treats an entry as live until `Date.now()`
is strictly greater — so an entry written and read within the same millisecond
*is* a hit. Scenarios that issue calls back to back would be served a frozen
price for a run of iterations, silently bypassing the providers they mean to
exercise, for a duration that depends on how fast the machine is.

Use `nonCachingPriceCache()` from the harness for any scenario whose subject is
provider behaviour. It backdates `expiresAt` so every lookup misses on every
machine. Use a real TTL only when the cache itself is what is under test.

## The harness

`oracle/tests/stress/harness.ts` provides:

- **`StressProvider`** — a price provider driveable into any failure mode, with
  a configurable latency profile (base, jitter, and a probabilistic slow tail).
- **`FailureMode`** — `NETWORK_ERROR`, `TIMEOUT`, `SERVER_ERROR`,
  `RATE_LIMITED`, `MALFORMED`, `STALE`, `BYZANTINE`. The last three are the
  interesting ones: they succeed at the transport layer and answer with bad
  data, so only validation catches them.
- **`generateShockPath`** — geometric price paths for spikes and crashes, so
  each tick is an equal percentage move, the way a cascade actually prints.
- **`driveLoad`** — bounded-concurrency load driver that records the full
  latency and success/failure distribution instead of aborting on first error.
- **`summarizeLatency`** / **`percentile`** — nearest-rank percentiles.
- **`nonCachingPriceCache`** — see above.

### Adding a scenario

```ts
import { StressProvider, FailureMode, driveLoad, summarizeLatency, recordScenario, nonCachingPriceCache } from './harness.js';

it('describes the property being protected', async () => {
  const provider = new StressProvider({ name: 'p1', priority: 1, weight: 1, seed: 42 })
    .setPrice('XLM', 0.15)
    .setFailure(0.5, FailureMode.SERVER_ERROR);

  const load = await driveLoad(100, 8, () => aggregator.getPrice('XLM'));
  const passed = /* the property you are asserting */;

  recordScenario({
    category: 'failure-scenarios',
    name: 'Short description shown in the report',
    passed,
    iterations: 100,
    successes: load.successes,
    failures: load.failures,
    latency: summarizeLatency(load.latencies),
    notes: { invariant: 'the property in one line' },
  });

  expect(passed).toBe(true);
});
```

Name the scenario after the property it protects, not the mechanism it pokes,
and always include an `invariant` note — the report is read by people deciding
whether a change is safe to ship, and a scenario whose purpose is unclear gets
ignored.

## Reporting

`npm run test:stress` writes to `oracle/stress-report/` (gitignored):

- `oracle-stress-report.json` — machine-readable, consumed by the CI gate.
- `oracle-stress-report.md` — per-category pass/fail tables, latency
  percentiles, and collapsible scenario detail.

Vitest runs each test file in its own worker, so scenarios append JSON-lines
shards to `stress-report/shards/` and the global teardown — which runs once in
the main process — merges them. Recording never throws: a reporting failure must
not fail the suite it is reporting on.

In CI, the markdown is written straight into the job summary, and the job fails
if the JSON reports any scenario as failed.

## Findings

Two issues surfaced while building this suite.

### Consensus anchoring in the aggregator (fixed)

`PriceValidator` keeps a last-accepted price per asset as its drift reference,
and `fetchFromAllProviders` handed it quotes in provider-priority order. The
first quote in a round therefore became the yardstick for its own peers.

A misconfigured or compromised highest-priority provider could exploit this: its
quote set the reference, the honest majority was then measured against that
reference, every honest quote was rejected as "deviating more than 10%", and the
liar's price survived as the aggregate — reported with single-source confidence
but no indication that consensus had been overruled.

`fetchFromAllProviders` now runs in three phases: collect every quote, screen the
round against its own median (dropping quotes that disagree by more than the
validator's threshold, and recording the disagreement against that provider's
circuit breaker), then validate the survivors most-consensus-aligned first. The
drift reference is now always a quote the round agreed on. Covered by
`oracle-failure.stress.test.ts` and `circuit-breaker.stress.test.ts`.

### Breaker cooldown versus volatility window (documented)

The volatility guard compares a new price against the last *accepted* one. When
a crash print is rejected, the last accepted price remains the pre-crash level.
If the breaker's cooldown expires while that pre-crash price is still inside the
volatility window, the next publication is measured against it, trips the breaker
again, and the asset cannot recover on its own.

Keep `breaker_cooldown_seconds >= VOLATILITY_WINDOW_SECONDS` (600s). Both the
working configuration and the hazard are pinned in
`tests/e2e/oracle-stress.e2e.test.ts`.

## Related

- [`MATH_LIBRARY.md`](./MATH_LIBRARY.md) — shared arithmetic used by the guards.
- `oracle/tests/failure-scenarios.test.ts` — single-failure unit coverage.
- `stellar-lend/contracts/hello-world/src/tests/oracle_circuit_breaker_test.rs` —
  per-guard unit coverage for the on-chain breaker.
