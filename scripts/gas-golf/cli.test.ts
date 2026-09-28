import { test } from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const run = promisify(execFile);
const HERE = path.dirname(fileURLToPath(import.meta.url));
const CLI = path.join(HERE, "index.ts");

interface CliResult {
  code: number;
  stdout: string;
  stderr: string;
}

/** Run the CLI the way a person or a CI job does. */
async function cli(...args: string[]): Promise<CliResult> {
  try {
    const { stdout, stderr } = await run(
      process.execPath,
      ["--experimental-strip-types", "--no-warnings", CLI, ...args],
      { cwd: path.resolve(HERE, "..", "..") },
    );
    return { code: 0, stdout, stderr };
  } catch (err) {
    const e = err as { code?: number; stdout: string; stderr: string };
    return { code: typeof e.code === "number" ? e.code : 1, stdout: e.stdout, stderr: e.stderr };
  }
}

// ── Listing ─────────────────────────────────────────────────────────────────

test("--list-challenges prints the open targets with their budgets", async () => {
  const { code, stdout } = await cli("--list-challenges");
  assert.equal(code, 0);
  assert.match(stdout, /lending::get_health_factor/);
  assert.match(stdout, /400000/);
  assert.match(stdout, /target\(s\)/);
});

test("--list-courses prints every gated course with a reference and vectors", async () => {
  const { code, stdout } = await cli("--list-courses");
  assert.equal(code, 0);
  assert.match(stdout, /Health factor computation/);
  assert.match(stdout, /Interest accrual on repay/);
  assert.match(stdout, /Liquidation sizing/);
});

test("--help is a usage error-free no-op", async () => {
  const { code, stdout } = await cli("--help");
  assert.equal(code, 0);
  assert.match(stdout, /Exit codes:/);
});

test("a bad --format is a usage error, not a crash", async () => {
  const { code, stderr } = await cli("--format", "toml");
  assert.equal(code, 2);
  assert.match(stderr, /--format must be text\|json\|markdown/);
});

test("a missing submissions file is a usage error", async () => {
  const { code, stderr } = await cli("--submissions", "submissions/nope.json");
  assert.equal(code, 2);
  assert.match(stderr, /no submissions index/);
});

// ── The default board: honest about having nothing to rank ──────────────────

test("the default board ranks nothing, says why, and still exits 0", async () => {
  // This is the state of the repository: no lending measurement exists. A tool
  // that printed zeros here, or exited non-zero, would be lying in one of the
  // two available directions.
  const { code, stdout, stderr } = await cli();
  assert.equal(code, 0, "an empty board is not a failure");
  assert.match(stdout, /No rankable entries\./);
  assert.match(stdout, /benchmark-results\.json does not exist/);
  assert.match(stdout, /no-measurement/);
  assert.match(stderr, /0 ranked, 3 not ranked/);
});

test("the default board never invents a score for the missing reference", async () => {
  const { stdout } = await cli("--format", "json");
  const board = JSON.parse(stdout) as {
    totals: { rankedEntries: number };
    challenges: { challenge: string; referenceInstructions: number | null }[];
  };
  assert.equal(board.totals.rankedEntries, 0);
  for (const challenge of board.challenges) {
    assert.equal(challenge.referenceInstructions, null);
  }
});

// ── The populated board, from the synthetic fixture ─────────────────────────

test("the fixture session ranks entries and pairs each against the reference", async () => {
  const { code, stdout } = await cli(
    "--submissions",
    "scripts/gas-golf/fixtures/submissions.json",
    "--reference",
    "scripts/gas-golf/fixtures/reference-session.json",
  );
  assert.equal(code, 0, "a correct submission must not fail the gate");
  assert.match(stdout, /lending::get_health_factor {2}\(budget 400,000 instructions\)/);
  assert.match(stdout, /201,338/);
  assert.match(stdout, /\+24\.99%/);
  assert.match(stdout, /0\.00%/, "the reference self-scores at zero");
  assert.match(stdout, /-28\.59%/, "a regression is shown as a negative, not hidden");
  assert.doesNotMatch(stdout, /Measurement integrity/, "a clean fixture session has no findings");
});

test("the JSON board is scriptable and records how it was scored", async () => {
  const { code, stdout } = await cli(
    "--submissions",
    "scripts/gas-golf/fixtures/submissions.json",
    "--reference",
    "scripts/gas-golf/fixtures/reference-session.json",
    "--format",
    "json",
  );
  assert.equal(code, 0);
  const board = JSON.parse(stdout) as {
    scoring: { metric: string; direction: string };
    challenges: { challenge: string; ranked: { submissionId: string; utilizationPct: number }[] }[];
  };
  assert.equal(board.scoring.metric, "utilizationPct");
  assert.equal(board.scoring.direction, "lower is better");
  const health = board.challenges.find((c) => c.challenge === "lending::get_health_factor");
  assert.deepEqual(health?.ranked.map((e) => e.submissionId), ["cached-threshold", "reference"]);
  // 201,338 / 400,000 * 100
  assert.equal(Number(health?.ranked[0].utilizationPct.toFixed(2)), 50.33);
});

test("the markdown board renders for a PR comment", async () => {
  const { code, stdout } = await cli(
    "--submissions",
    "scripts/gas-golf/fixtures/submissions.json",
    "--reference",
    "scripts/gas-golf/fixtures/reference-session.json",
    "--format",
    "markdown",
  );
  assert.equal(code, 0);
  assert.match(stdout, /^## Gas golf leaderboard/);
  assert.match(stdout, /\| 1 \| `cached-threshold` \|/);
});

test("--out writes the report to a file", async () => {
  const out = path.join(HERE, ".tmp-board.md");
  try {
    const { code, stderr } = await cli("--format", "markdown", "--out", out);
    assert.equal(code, 0);
    assert.match(stderr, /Wrote markdown board/);
    const written = await (await import("node:fs/promises")).readFile(out, "utf8");
    assert.match(written, /## Gas golf leaderboard/);
  } finally {
    (await import("node:fs")).rmSync(out, { force: true });
  }
});

// ── The gate, end to end ────────────────────────────────────────────────────

test("--gate all passes every entry in the competition index", async () => {
  const { code, stdout } = await cli("--gate", "all");
  assert.equal(code, 0, stdout);
  assert.doesNotMatch(stdout, /REJECT/);
  assert.match(stdout, /PASS/);
});

test("--gate rejects the exploit and exits 1", async () => {
  const { code, stdout } = await cli("--submissions", "submissions/invalid.json", "--gate", "all");
  assert.equal(code, 1, "a failing gate must be visible to CI");
  assert.match(stdout, /collapsed-division\s+REJECT/);
  assert.match(stdout, /diverges from/);
});

test("the board run fails when any submission fails the gate", async () => {
  // Correctness gates the score, so a bad entry must not be quietly ranked
  // alongside good ones.
  const { code, stderr } = await cli("--submissions", "submissions/invalid.json");
  assert.equal(code, 1);
  assert.match(stderr, /failed the correctness gate: collapsed-division \(diverged\)/);
});

test("the exploit is ranked nowhere even with a measurement", async () => {
  // The decisive property: a fast, measured, wrong implementation still does not
  // appear on the board.
  const forged = path.join(HERE, ".tmp-forged.json");
  const forgedIndex = path.join(HERE, ".tmp-forged-index.json");
  await (await import("node:fs/promises")).writeFile(
    forgedIndex,
    JSON.stringify([
      {
        id: "forged-optimization",
        author: "@cheater",
        challenge: "lending::get_health_factor",
        implementation: "submissions/collapsed-division.ts",
        measurement: "fixtures/measurements/cached-threshold.json",
      },
    ]),
  );
  try {
    const { code, stdout } = await cli(
      "--submissions",
      forgedIndex,
      "--reference",
      "fixtures/reference-session.json",
      "--format",
      "json",
    );
    assert.equal(code, 1);
    const board = JSON.parse(stdout) as {
      totals: { rankedEntries: number };
      challenges: { ranked: unknown[] }[];
    };
    assert.equal(board.totals.rankedEntries, 0, "nothing may be ranked without passing the gate");
    for (const challenge of board.challenges) assert.deepEqual(challenge.ranked, []);
    assert.match(stdout, /gate-failed/);
  } finally {
    const fs = await import("node:fs");
    fs.rmSync(forged, { force: true });
    fs.rmSync(forgedIndex, { force: true });
  }
});

// ── Integrity ───────────────────────────────────────────────────────────────

test("--require-fresh rejects a session stitched together from two commits", async () => {
  const ts = "2026-01-01T00:00:00Z";
  const report = path.join(HERE, ".tmp-stale.json");
  await (await import("node:fs/promises")).writeFile(
    report,
    JSON.stringify({
      timestamp: ts,
      results: [
        { operation: "lending::get_health_factor", instructions: 200_000, memory_bytes: 9_000, git_commit: "aaa" },
        { operation: "lending::repay", instructions: 700_000, memory_bytes: 20_000, git_commit: "bbb" },
      ],
    }),
  );
  try {
    const lenient = await cli("--reference", report);
    assert.equal(lenient.code, 0, "freshness is opt-in");
    const strict = await cli("--reference", report, "--require-fresh");
    assert.equal(strict.code, 0, "a stale report explains itself rather than crashing");
    assert.match(strict.stdout, /stale-pairing/);
    assert.match(strict.stdout, /2 different commits/);
  } finally {
    (await import("node:fs")).rmSync(report, { force: true });
  }
});

test("a report of zeroes is treated as unmeasured, not as free", async () => {
  const report = path.join(HERE, ".tmp-zeroes.json");
  await (await import("node:fs/promises")).writeFile(
    report,
    JSON.stringify({
      timestamp: "2026-01-01T00:00:00Z",
      results: [
        { operation: "lending::get_health_factor", instructions: 0, memory_bytes: 0 },
        { operation: "lending::repay", instructions: 0, memory_bytes: 0 },
      ],
    }),
  );
  try {
    const { code, stdout } = await cli("--reference", report, "--submissions", "fixtures/submissions.json");
    assert.match(stdout, /zero-instructions/);
    assert.match(stdout, /which means unmeasured rather than free/);
    assert.equal(code, 0, "zero rows are reported, not crashed on");
  } finally {
    (await import("node:fs")).rmSync(report, { force: true });
  }
});
