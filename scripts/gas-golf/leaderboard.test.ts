import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import {
  OPERATION_TYPES,
  classifyOperation,
  findChallenge,
  loadCourse,
  loadCourseFromDisk,
  parseBudgets,
  parsePublicFunctions,
  splitId,
  type Course,
} from "./challenges.ts";
import {
  BLOCKING_REASONS,
  checkIntegrity,
  hasBlockingFinding,
  indexById,
  parseMeasurements,
} from "./measurements.ts";
import { renderMarkdown, renderText, toJson, type BoardContext } from "./board.ts";
import { buildLeaderboard, implausibleSaving, rankChallenge, savingsPct, utilizationPct, type ScoreInput } from "./scoring.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");
const BENCHMARKS = path.join(REPO_ROOT, "stellar-lend/benchmarks");

const PUBLIC_FUNCTIONS = JSON.stringify({
  version: 1,
  required_operations: [
    "lending::deposit",
    "lending::deposit_warm",
    "lending::borrow",
    "lending::get_health_factor",
    "lending::liquidate",
    "lending::set_pause",
    "amm::execute_swap",
    "amm::get_asset_config",
  ],
});

const BUDGETS = JSON.stringify({
  gas_budgets: {
    "lending::deposit": 800_000,
    "lending::borrow": 1_200_000,
    "lending::get_health_factor": 400_000,
    "lending::liquidate": 1_500_000,
    "lending::set_pause": 200_000,
    "amm::execute_swap": 120_000,
  },
  operation_type_budgets: {
    read: 400_000,
    admin: 500_000,
    user_write: 1_200_000,
    liquidation: 1_500_000,
    flash_loan: 1_800_000,
    batch: 3_000_000,
  },
});

function course(): Course {
  return loadCourse({
    publicFunctions: PUBLIC_FUNCTIONS,
    budgets: BUDGETS,
    paths: { publicFunctions: "public-functions.json", budgets: "baseline.json" },
  });
}

// ── Challenges ───────────────────────────────────────────────────────────────

test("parsePublicFunctions reads the required-operation list", () => {
  assert.deepEqual(parsePublicFunctions(PUBLIC_FUNCTIONS).length, 8);
});

test("parsePublicFunctions rejects a malformed list rather than scoring it partially", () => {
  assert.throws(() => parsePublicFunctions("{}"), /required_operations/);
  assert.throws(() => parsePublicFunctions(JSON.stringify({ required_operations: ["no-contract"] })), /contract::op/);
  assert.throws(() => parsePublicFunctions("not json"), /JSON/);
});

test("parsePublicFunctions de-duplicates rather than double-counting a target", () => {
  const parsed = parsePublicFunctions(
    JSON.stringify({ required_operations: ["lending::deposit", "lending::deposit"] }),
  );
  assert.deepEqual(parsed, ["lending::deposit"]);
});

test("parseBudgets reads both budget blocks and drops non-positive entries", () => {
  const budgets = parseBudgets(
    JSON.stringify({ gas_budgets: { "lending::deposit": 800_000, "lending::broken": 0 }, operation_type_budgets: { read: 400_000 } }),
  );
  assert.equal(budgets.perOperation["lending::deposit"], 800_000);
  assert.equal(budgets.perOperation["lending::broken"], undefined, "a zero budget is not a budget");
  assert.equal(budgets.perType.read, 400_000);
  assert.deepEqual(parseBudgets("{}"), { perOperation: {}, perType: {} });
});

test("classifyOperation matches the gas report's six operation types", () => {
  assert.equal(classifyOperation("deposit"), "user_write");
  assert.equal(classifyOperation("batch_deposit"), "batch", "batch_ wins, as in report.ts");
  assert.equal(classifyOperation("get_health_factor"), "read");
  assert.equal(classifyOperation("get_max_liquidatable_amount"), "read", "a view is a read, not a liquidation");
  assert.equal(classifyOperation("can_borrow"), "read");
  assert.equal(classifyOperation("set_oracle"), "admin");
  assert.equal(classifyOperation("initialize"), "admin");
  assert.equal(classifyOperation("liquidate"), "liquidation");
  assert.equal(classifyOperation("execute_flash_loan"), "flash_loan");
  for (const op of OPERATION_TYPES) {
    assert.ok(OPERATION_TYPES.includes(op));
  }
});

test("a warm twin shares the budget of the operation it warms", () => {
  // `framework.rs` and `gas_benchmark_report.py` both key `deposit_warm` under
  // `lending::deposit`'s budget, so the course has to agree or the two rank
  // differently.
  const built = course();
  const cold = findChallenge(built, "lending::deposit");
  const warm = findChallenge(built, "lending::deposit_warm");
  assert.equal(cold.budget, 800_000);
  assert.equal(warm.budget, 800_000, "the warm twin must not fall back to the type budget");
  assert.equal(warm.budgetSource, "operation");
});

test("an operation with no own budget falls back to its type budget", () => {
  const built = course();
  const read = findChallenge(built, "amm::get_asset_config");
  assert.equal(read.budget, 400_000);
  assert.equal(read.budgetSource, "operation-type");
  // `get_` wins over everything in classifyOperation, so this is a read.
  assert.equal(read.operationType, "read");
  assert.equal(read.budget, 400_000, "the read type budget, not the 500k admin one");
});

test("an operation with no budget at all is reported unscoreable, not silently zero", () => {
  const built = loadCourse({
    publicFunctions: PUBLIC_FUNCTIONS,
    // No per-operation budgets, and a type budget block with no `read` entry.
    budgets: JSON.stringify({ gas_budgets: { "lending::deposit": 800_000 }, operation_type_budgets: {} }),
    paths: { publicFunctions: "p", budgets: "b" },
  });
  assert.ok(built.unscored.includes("lending::get_health_factor"));
  assert.equal(findChallenge(built, "lending::get_health_factor").scoreable, false);
  assert.equal(findChallenge(built, "lending::deposit").scoreable, true);
});

test("findChallenge lists the lending targets when given an unknown id", () => {
  assert.throws(() => findChallenge(course(), "lending::nope"), /Lending challenges:.*lending::deposit/);
});

test("splitId tolerates the scenario-suffixed measurement key form", () => {
  assert.deepEqual(splitId("lending::deposit"), { contract: "lending", fn: "deposit", scenario: null });
  // The suffix is a *scenario* tag, not a function name.
  assert.deepEqual(splitId("lending::deposit [warm]"), { contract: "lending", fn: "deposit", scenario: "warm" });
  // A bare name has no contract, so the caller supplies it.
  assert.deepEqual(splitId("deposit_collateral"), { contract: "", fn: "deposit_collateral", scenario: null });
  assert.deepEqual(splitId("amm::get_asset_config"), { contract: "amm", fn: "get_asset_config", scenario: null });
});

// ── Measurements ─────────────────────────────────────────────────────────────

const BENCH_REPORT = JSON.stringify({
  version: "0.1.0",
  timestamp: "2026-09-01T00:00:00Z",
  results: [
    {
      operation: "lending::deposit",
      contract: "lending",
      instructions: 400_000,
      memory_bytes: 58_682,
      storage_reads: 1,
      storage_writes: 2,
      cold_storage: true,
      git_commit: "abc1234",
    },
    {
      operation: "lending::borrow",
      contract: "lending",
      instructions: 500_000,
      memory_bytes: 32_789,
      storage_reads: 3,
      storage_writes: 3,
      cold_storage: true,
      git_commit: "abc1234",
    },
  ],
});

test("parseMeasurements reads the BenchmarkReport shape", () => {
  const set = parseMeasurements(BENCH_REPORT);
  assert.equal(set.shape, "benchmark-report");
  assert.equal(set.reportedAt, "2026-09-01T00:00:00Z");
  assert.equal(set.measurements.length, 2);
  const deposit = set.measurements[0];
  assert.equal(deposit.id, "lending::deposit");
  assert.equal(deposit.instructions, 400_000);
  assert.equal(deposit.memoryBytes, 58_682);
  assert.equal(deposit.commit, "abc1234");
  // Declared, not measured — carried for display, never a ranking input.
  assert.equal(deposit.declaredStorageWrites, 2);
});

test("parseMeasurements reads the gas-baseline shape and namespaces it", () => {
  const set = parseMeasurements(
    JSON.stringify({
      version: 1,
      contract: "hello-world",
      benchmarks: [{ operation: "deposit_collateral", scenario: "write_cold", cpu_insns: 354_765, mem_bytes: 58_682 }],
    }),
  );
  assert.equal(set.shape, "gas-baseline");
  assert.equal(set.measurements[0].id, "hello_world::deposit_collateral [write_cold]");
  assert.equal(set.measurements[0].instructions, 354_765);
});

test("parseMeasurements reports an unrecognised shape rather than an empty set", () => {
  const set = parseMeasurements(JSON.stringify({ something: "else" }));
  assert.equal(set.shape, "unknown");
  assert.equal(set.measurements.length, 0);
  assert.throws(() => parseMeasurements("not json"), /not valid JSON/);
});

test("an instruction count of zero is an integrity failure, not a fast submission", () => {
  // `coverage_failures` in gas_benchmark_report.py is set-membership only, so a
  // file of zeroes passes every existing gate in the repository.
  const set = parseMeasurements(
    JSON.stringify({
      results: [{ operation: "lending::deposit", contract: "lending", instructions: 0 }],
    }),
  );
  const findings = checkIntegrity(set, course().challenges);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].reason, "zero-instructions");
  assert.ok(findings[0].detail.includes("unmeasured rather than free"));
  assert.equal(hasBlockingFinding(findings), true);
});

test("an empty or unreadable measurement set blocks scoring", () => {
  const empty = parseMeasurements(JSON.stringify({ results: [] }));
  assert.equal(checkIntegrity(empty, [])[0].reason, "empty");
  assert.equal(hasBlockingFinding(checkIntegrity(empty, [])), true);

  const unknown = parseMeasurements(JSON.stringify({ nothing: 1 }));
  assert.equal(checkIntegrity(unknown, [])[0].reason, "unreadable");
});

test("a measurement for an operation that is not an open target is flagged", () => {
  const set = parseMeasurements(
    JSON.stringify({ results: [{ operation: "lending::brand_new", contract: "lending", instructions: 10 }] }),
  );
  const findings = checkIntegrity(set, course().challenges);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].reason, "not-required");
  // Non-blocking: it is a scope note, not a reason to refuse the whole set.
  assert.equal(hasBlockingFinding(findings), false);
});

test("measurements spanning several commits are rejected when a fresh pair is required", () => {
  const set = parseMeasurements(
    JSON.stringify({
      results: [
        { operation: "lending::deposit", contract: "lending", instructions: 10, git_commit: "aaa" },
        { operation: "lending::borrow", contract: "lending", instructions: 10, git_commit: "bbb" },
      ],
    }),
  );
  assert.equal(hasBlockingFinding(checkIntegrity(set, [])), false, "not checked by default");
  const fresh = checkIntegrity(set, [], { requireFresh: true });
  const pairing = fresh.find((f) => f.reason === "stale-pairing");
  assert.ok(pairing, `expected a stale-pairing finding, got ${fresh.map((f) => f.reason).join(", ")}`);
  assert.match(pairing.detail, /2 different commits/);
  assert.equal(hasBlockingFinding(fresh), true);
});

test("a report whose rows name no commit is refused, not passed, when freshness is required", () => {
  // The exact shape `write_json` (`stellar-lend/benchmarks/src/report.rs:195-204`)
  // produces: a report-level timestamp and per-row timestamps, but no commit.
  // Collecting commits from it yields an empty set, so "more than one commit" is
  // false — which used to read as a clean bill of health on every real report.
  const set = parseMeasurements(
    JSON.stringify({
      version: "0.1.0",
      timestamp: "2026-09-28T10:00:00Z",
      total_benchmarks: 2,
      results: [
        {
          operation: "lending::deposit",
          contract: "lending",
          instructions: 400_000,
          memory_bytes: 58_682,
          timestamp: "2026-09-28T10:00:00.100Z",
        },
        {
          operation: "lending::borrow",
          contract: "lending",
          instructions: 900_000,
          memory_bytes: 61_000,
          timestamp: "2026-09-28T10:00:00.900Z",
        },
      ],
    }),
  );
  assert.equal(set.measurements.length, 2);
  assert.ok(
    set.measurements.every((m) => m.commit === null),
    "the premise: the real report writer emits no per-row commit",
  );

  // Default behaviour is unchanged — freshness is opt-in, and an unverifiable
  // report is still scoreable when nobody asked for the proof.
  assert.equal(
    checkIntegrity(set, course().challenges).some((f) => f.reason === "no-provenance"),
    false,
    "not checked by default",
  );

  const strict = checkIntegrity(set, course().challenges, { requireFresh: true });
  const finding = strict.find((f) => f.reason === "no-provenance");
  assert.ok(finding, `expected a no-provenance finding, got ${strict.map((f) => f.reason).join(", ")}`);
  assert.match(finding.detail, /2 of 2 row\(s\) carry no commit/);
  assert.match(finding.detail, /report\.rs:195-204/);
  assert.equal(hasBlockingFinding(strict), true, "an unverifiable session blocks rather than passes");
});

test("a partly attributed report is refused too, because the unattributed rows are the hole", () => {
  // One committed row next to one uncommitted row is what a stitched report
  // looks like when only part of it was re-stamped: a single commit is present,
  // so the commit count looks clean, and the uncommitted row rides along.
  const set = parseMeasurements(
    JSON.stringify({
      results: [
        { operation: "lending::deposit", contract: "lending", instructions: 10, git_commit: "aaa" },
        { operation: "lending::borrow", contract: "lending", instructions: 10 },
      ],
    }),
  );
  const strict = checkIntegrity(set, [], { requireFresh: true });
  assert.ok(strict.some((f) => f.reason === "no-provenance"), "the uncommitted row is caught");
  assert.equal(
    strict.some((f) => f.reason === "stale-pairing"),
    false,
    "one commit is not two commits; the missing row is the separate failure",
  );
  assert.equal(hasBlockingFinding(strict), true);
});

test("BLOCKING_REASONS is the set that refuses to rank", () => {
  assert.deepEqual(
    [...BLOCKING_REASONS].sort(),
    ["empty", "no-provenance", "stale-pairing", "unreadable", "zero-instructions"],
  );
});

test("indexById keeps the first row for a duplicated id", () => {
  const set = parseMeasurements(
    JSON.stringify({
      results: [
        { operation: "lending::deposit", contract: "lending", instructions: 1 },
        { operation: "lending::deposit", contract: "lending", instructions: 2 },
      ],
    }),
  );
  assert.equal(indexById(set).get("lending::deposit")?.instructions, 1);
});

// ── Scoring ──────────────────────────────────────────────────────────────────

function score(over: Partial<ScoreInput> = {}): ScoreInput {
  return {
    submissionId: "s",
    author: "@a",
    challenge: "lending::deposit",
    gatePassed: true,
    instructions: 400_000,
    memoryBytes: 50_000,
    referenceInstructions: 500_000,
    budget: 800_000,
    ...over,
  };
}

test("utilizationPct is the gas report's own primitive", () => {
  assert.equal(utilizationPct(400_000, 800_000), 50);
  assert.equal(utilizationPct(0, 800_000), null, "zero is unmeasured, not perfect");
  assert.equal(utilizationPct(400_000, 0), null, "no budget is unscoreable, not free");
});

test("savingsPct needs a same-session reference", () => {
  assert.equal(savingsPct(400_000, 500_000), 20);
  assert.equal(savingsPct(600_000, 500_000), -20, "a regression is negative");
  assert.equal(savingsPct(400_000, null), null, "an absolute number is not a score");
  assert.equal(savingsPct(400_000, 0), null);
});

test("a saving beyond 100% is implausible rather than excellent", () => {
  assert.match(String(implausibleSaving(150)), /impossible/);
  assert.match(String(implausibleSaving(-150)), /negative instruction count/);
  assert.equal(implausibleSaving(99), null);
  assert.equal(implausibleSaving(-50), null, "a regression is allowed, just not good");
  assert.equal(implausibleSaving(null), null);
});

test("a failed gate disqualifies regardless of the measurement", () => {
  const entry = rankChallenge("lending::deposit", 800_000, [score({ gatePassed: false })]).entries[0];
  assert.equal(entry.disqualified, "gate-failed");
  assert.deepEqual(rankChallenge("lending::deposit", 800_000, [score({ gatePassed: false })]).ranked, []);
});

test("a submission with no measurement or no reference cannot be ranked", () => {
  assert.equal(rankChallenge("c", 800_000, [score({ instructions: null })]).entries[0].disqualified, "no-measurement");
  assert.equal(rankChallenge("c", 800_000, [score({ instructions: 0 })]).entries[0].disqualified, "unreadable-measurement");
  assert.equal(
    rankChallenge("c", 800_000, [score({ referenceInstructions: null })]).entries[0].disqualified,
    "no-reference",
  );
  assert.equal(rankChallenge("c", null, [score({ budget: null })]).entries[0].disqualified, "no-budget");
});

test("entries rank by utilisation, then memory, then id", () => {
  const board = rankChallenge("lending::deposit", 800_000, [
    score({ submissionId: "b", instructions: 200_000 }),
    score({ submissionId: "a", instructions: 100_000 }),
    score({ submissionId: "c", instructions: 200_000, memoryBytes: 10_000 }),
  ]);
  assert.deepEqual(board.ranked.map((e) => e.submissionId), ["a", "c", "b"]);
  assert.equal(board.best?.submissionId, "a");
  assert.equal(board.referenceInstructions, null, "with no session reference there is no paired figure");
});

test("the session reference is passed in, not inferred from a ranked entry", () => {
  // If it were inferred, the first-ranked submission would become the thing
  // every other submission is compared against.
  const board = rankChallenge(
    "lending::deposit",
    800_000,
    [score({ submissionId: "a", instructions: 100_000, referenceInstructions: 999_000 })],
    500_000,
  );
  assert.equal(board.referenceInstructions, 500_000);
});

test("buildLeaderboard takes the reference per challenge", () => {
  const board = buildLeaderboard(
    [score({ submissionId: "a", instructions: 100_000, referenceInstructions: 400_000 })],
    { "lending::deposit": 400_000 },
  );
  assert.equal(board.challenges[0].referenceInstructions, 400_000);
  assert.equal(board.challenges[0].ranked[0].savingsPct, 75);
});

test("unrankable entries are collected rather than shown on the board", () => {
  const leaderboard = buildLeaderboard([
    score({ submissionId: "good", instructions: 100_000 }),
    score({ submissionId: "bad", gatePassed: false }),
  ]);
  assert.equal(leaderboard.totals.rankedEntries, 1);
  assert.equal(leaderboard.totals.disqualifiedEntries, 1);
  assert.deepEqual(leaderboard.unscored.map((e) => e.submissionId), ["bad"]);
  assert.equal(leaderboard.totals.bestOverall?.submissionId, "good");
});

test("an empty input produces an empty board rather than a crash", () => {
  const board = buildLeaderboard([]);
  assert.deepEqual(board.challenges, []);
  assert.equal(board.totals.rankedEntries, 0);
  assert.equal(board.totals.bestOverall, null);
});

// ── Board rendering ──────────────────────────────────────────────────────────

function context(over: Partial<BoardContext> = {}): BoardContext {
  return {
    course: { sources: { publicFunctions: "public-functions.json", budgets: "baseline.json" } },
    unscoredChallenges: [],
    integrity: [],
    measurementSource: "benchmark-results.json",
    measurementShape: "benchmark-report",
    measurementTimestamp: "2026-09-01T00:00:00Z",
    submissionsConsidered: 2,
    ...over,
  };
}

test("an empty board explains itself instead of printing a table", () => {
  const text = renderText(buildLeaderboard([]), context({
    measurementSource: "benchmark-results.json (absent)",
    measurementShape: "none",
    integrity: [
      {
        reason: "empty",
        id: "",
        detail: "benchmark-results.json does not exist. Run ./run-benchmarks.sh to produce a report.",
      },
    ],
  }));
  assert.ok(text.includes("No rankable entries"));
  assert.ok(text.includes("Why there is nothing to rank"));
  assert.ok(text.includes("run-benchmarks.sh"));
  assert.ok(!text.includes("Rank  Submission"), "no table header when there is nothing to rank");
});

test("an empty board counts rejection reasons rather than hiding them", () => {
  const text = renderText(
    buildLeaderboard([score({ submissionId: "x", gatePassed: false }), score({ submissionId: "y", instructions: null })]),
    context(),
  );
  assert.ok(text.includes("Every entry was rejected, by reason:"));
  assert.ok(text.includes("gate-failed"));
  assert.ok(text.includes("no-measurement"));
});

test("an empty board names challenges that have no budget", () => {
  const text = renderText(buildLeaderboard([]), context({ unscoredChallenges: ["lending::mystery"] }));
  assert.ok(text.includes("unbudgeted rather than un-gated"));
});

test("a populated board names the score formula and the author", () => {
  const board = buildLeaderboard([
    score({ submissionId: "fast", instructions: 300_000, referenceInstructions: 600_000 }),
  ]);
  const text = renderText(board, context());
  assert.ok(text.includes("utilizationPct") === false, "the text table does not leak the internal name");
  assert.ok(text.includes("fast"));
  assert.ok(text.includes("@a"));
  assert.ok(text.includes("+50.00%"), "the paired saving must be shown");
  assert.ok(text.includes("Measurement integrity") === false, "no findings, no section");

  const md = renderMarkdown(board, context());
  assert.ok(md.startsWith("## Gas golf leaderboard"));
  assert.ok(md.includes("instructions / budget"));
  assert.match(md, /lower is better/i);
  assert.ok(md.includes("| 1 | `fast` |"));
});

test("an implausible saving is flagged in both renderers", () => {
  // A saving above 100% is unreachable through `savingsPct` (instructions cannot
  // be negative once `utilizationPct` has admitted the entry), so it is
  // defence-in-depth and is unit-tested directly. The reachable case is a
  // regression past -100%, i.e. a submission costing more than twice the
  // reference.
  const board = buildLeaderboard([
    score({ submissionId: "three-times-slower", instructions: 1_500_000, referenceInstructions: 500_000 }),
  ]);
  assert.equal(board.challenges[0].ranked[0].savingsPct, -200);
  assert.ok(renderText(board, context()).includes("negative instruction count"));
  assert.ok(renderMarkdown(board, context()).includes("negative instruction count"));
});

test("integrity findings are surfaced in both renderers", () => {
  const ctx = context({
    integrity: [{ reason: "zero-instructions", id: "lending::deposit", detail: "reports 0 instructions" }],
  });
  const board = buildLeaderboard([score({ submissionId: "ok", instructions: 100_000 })]);
  assert.ok(renderText(board, ctx).includes("Measurement integrity"));
  assert.ok(renderMarkdown(board, ctx).includes("Measurement integrity"));
  assert.ok(renderMarkdown(board, ctx).includes("zero-instructions"));
});

test("a blocking reason the renderers do not know about is still disclosed", () => {
  // `explainNoRanking` used to restate the blocking list inline, so a reason
  // added to `BLOCKING_REASONS` would be scored correctly and then not printed
  // where the empty state points the reader. Driven off the constant: the next
  // reason added cannot be quietly dropped.
  for (const reason of BLOCKING_REASONS) {
    const ctx = context({ integrity: [{ reason, id: "lending::deposit", detail: "detail for " + reason }] });
    const board = buildLeaderboard([score({ submissionId: "x", instructions: null })]);
    for (const rendered of [renderText(board, ctx), renderMarkdown(board, ctx)]) {
      assert.ok(
        rendered.includes(`Why there is nothing to rank`),
        `${reason} is blocking, so the empty state must explain itself`,
      );
      assert.ok(rendered.includes(reason), `${reason} must be named in the disclosure`);
    }
  }
});

test("the JSON board round-trips and records how it was scored", () => {
  const board = buildLeaderboard([score({ submissionId: "fast", instructions: 300_000, referenceInstructions: 600_000 })]);
  const json = toJson(board, context());
  const roundTripped = JSON.parse(JSON.stringify(json));
  assert.deepEqual(roundTripped, json);
  const parsed = json as { scoring: { metric: string; direction: string; note: string } };
  assert.equal(parsed.scoring.metric, "utilizationPct");
  assert.equal(parsed.scoring.direction, "lower is better");
  assert.match(parsed.scoring.note, /declared, not measured/);
});

// ── The repository's own data ────────────────────────────────────────────────

test("the real course has 82 targets and every one has a budget", () => {
  const built = loadCourseFromDisk(REPO_ROOT);
  assert.equal(built.challenges.length, 82, "public-functions.json declares 82 required operations");
  assert.deepEqual(built.unscored, [], "every required operation resolves to a budget");
  assert.equal(built.challenges.filter((c) => c.contract === "lending").length, 22);
});

test("every golf course targets an operation that is an open target", () => {
  // A course on an operation outside public-functions.json would be un-gatable:
  // it would not appear in the coverage check or carry a budget.
  const built = loadCourseFromDisk(REPO_ROOT);
  const targets = new Set(built.challenges.map((c) => c.id));
  for (const challenge of ["lending::get_health_factor", "lending::repay", "lending::liquidate"]) {
    assert.ok(targets.has(challenge), `${challenge} is not an open target`);
    assert.equal(findChallenge(built, challenge).scoreable, true);
  }
});

test("the golf budgets are the ones the benchmark framework already declares", () => {
  const built = loadCourseFromDisk(REPO_ROOT);
  const framework = fs.readFileSync(path.join(BENCHMARKS, "src/framework.rs"), "utf8");
  assert.match(framework, /"lending::get_health_factor"\.into\(\), 400_000/);
  assert.match(framework, /"lending::repay"\.into\(\), 1_000_000/);
  assert.match(framework, /"lending::liquidate"\.into\(\), 1_500_000/);
  assert.equal(findChallenge(built, "lending::get_health_factor").budget, 400_000);
  assert.equal(findChallenge(built, "lending::repay").budget, 1_000_000);
  assert.equal(findChallenge(built, "lending::liquidate").budget, 1_500_000);
});

test("the committed baseline carries no results, so the board must not render a table", () => {
  // This is the state that makes the empty-state honesty load-bearing: every
  // gas gate in the repository currently passes vacuously.
  const baseline = JSON.parse(fs.readFileSync(path.join(BENCHMARKS, "baseline.json"), "utf8")) as {
    results: unknown[];
    gas_budgets: Record<string, number>;
  };
  assert.deepEqual(baseline.results, [], "baseline.json still has an empty results array");
  assert.ok(Object.keys(baseline.gas_budgets).length > 0, "the budgets are what the course reads");

  const set = parseMeasurements(fs.readFileSync(path.join(BENCHMARKS, "baseline.json"), "utf8"));
  const findings = checkIntegrity(set, loadCourseFromDisk(REPO_ROOT).challenges);
  assert.equal(findings[0].reason, "empty");
  assert.equal(hasBlockingFinding(findings), true);
});

test("the committed gas-baseline has no lending measurement to rank", () => {
  const set = parseMeasurements(fs.readFileSync(path.join(BENCHMARKS, "gas-baseline.json"), "utf8"));
  assert.equal(set.shape, "gas-baseline");
  assert.ok(set.measurements.length > 0, "there are real measurements");
  assert.equal(
    set.measurements.filter((m) => m.contract === "lending").length,
    0,
    "all of them are hello-world, so no lending entry can be scored",
  );
});

test("the scratch directory helper stays outside the repository", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "gas-golf-"));
  try {
    assert.ok(!path.resolve(dir).startsWith(path.resolve(REPO_ROOT)));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
