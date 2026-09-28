# Synthetic fixtures

**The instruction counts in this directory are invented.** They exist so the
board's populated path — the one that actually ranks something — can be run,
reviewed and tested without a benchmark run. No number here came out of
`env.cost_estimate()`. Do not quote them, do not put them in a PR comment, and
do not treat a board built from them as a result.

The real path is the opposite: `stellar-lend/benchmark-results.json` does not
exist on this branch, and `stellar-lend/benchmarks/baseline.json` has
`"results": []`, so the default board run has nothing to rank and says so.

```
# The honest default: no measurements, nothing ranked, exit 0.
node --experimental-strip-types scripts/gas-golf/index.ts

# The populated path, from invented numbers.
node --experimental-strip-types scripts/gas-golf/index.ts \
  --submissions scripts/gas-golf/fixtures/submissions.json \
  --reference scripts/gas-golf/fixtures/reference-session.json

# The gate catching the worked exploit, exit 1.
node --experimental-strip-types scripts/gas-golf/index.ts \
  --submissions scripts/gas-golf/submissions/invalid.json
```

Every fixture file carries a `_comment` saying it is synthetic, and all rows in
`reference-session.json` share one commit — a score is only meaningful within a
single session, and `--require-fresh` enforces that.
