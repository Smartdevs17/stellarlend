import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import { courseIds, gateAll, gateSubmission, getCourse, type Submission } from "./gate.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));

function submission(over: Partial<Submission> = {}): Submission {
  return {
    id: "example",
    author: "@example",
    challenge: "lending::get_health_factor",
    implementation: "submissions/cached-health-factor.ts",
    ...over,
  };
}

// ── The gate is the point of the tool ────────────────────────────────────────

test("every course's reference agrees with itself over its full vector set", async () => {
  for (const id of courseIds()) {
    const course = getCourse(id)!;
    const result = await gateSubmission(
      submission({ challenge: id, implementation: course.reference.replace("courses/", "courses/") }),
    );
    assert.equal(result.passed, true, `${id}: ${result.detail}`);
    assert.ok(result.vectors > 0, `${id} has no vectors`);
    assert.equal(result.reason, null);
  }
});

test("a course is rejected for an operation that is not an open target", async () => {
  const result = await gateSubmission(submission({ challenge: "lending::not_a_course" }));
  assert.equal(result.passed, false);
  assert.equal(result.reason, "unknown-challenge");
});

test("a submission that carries its own budget is rejected", async () => {
  // `framework.rs`'s `get_budget()` returns 0 for an unknown operation and
  // `budget == 0` means unbudgeted, so a self-supplied budget is how an
  // un-gatable operation would sneak in.
  const result = await gateSubmission(submission({ budget: 1 }));
  assert.equal(result.passed, false);
  assert.equal(result.reason, "budget-override");
  assert.match(result.detail, /maintainer-owned/);
});

test("a submission with no implementation file is rejected, not scored as free", async () => {
  const result = await gateSubmission(submission({ implementation: "submissions/does-not-exist.ts" }));
  assert.equal(result.passed, false);
  assert.equal(result.reason, "implementation-unreadable");
});

test("an implementation that does not export run() is rejected", async () => {
  const dir = fs.mkdtempSync(path.join(import.meta.dirname, ".tmp-impl-"));
  try {
    fs.writeFileSync(path.join(dir, "broken.ts"), "export const name = 'broken';\n");
    const result = await gateSubmission(submission({ implementation: path.join(dir, "broken.ts") }));
    assert.equal(result.passed, false);
    assert.equal(result.reason, "implementation-unreadable");
    assert.match(result.detail, /must export async run\(scenario\)/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

// ── The health factor course: the worked exploit ────────────────────────────

test("the collapsed-division exploit is caught on exactly the truncation vectors", async () => {
  const result = await gateSubmission(submission({ implementation: "submissions/collapsed-division.ts" }));
  assert.equal(result.passed, false);
  assert.equal(result.reason, "diverged");
  assert.equal(result.vectors, 10, "all ten vectors are compared, not just the failing one");
  assert.ok(result.divergences.length > 0);
  // The divergence must be on the health factor itself, not a diagnostic field.
  assert.ok(
    result.divergences.some((d) => d.path.endsWith("healthFactor")),
    `expected a healthFactor divergence, got ${result.divergences.map((d) => d.path).join(", ")}`,
  );
  assert.match(result.detail, /diverges from/);
});

test("the collapsed-division divergence straddles the liquidation boundary", async () => {
  // This is why the exploit is dangerous rather than merely wrong: at
  // collateral 1501 / threshold 6667 the contract's two truncations give exactly
  // 10_000 (on the boundary) and the collapsed form gives 10_007 (comfortably
  // above it), so the rewrite would declare a liquidatable position healthy.
  const course = getCourse("lending::get_health_factor")!;
  const reference = await import(path.resolve(HERE, course.reference));
  const collapsed = await import(path.resolve(HERE, "submissions/collapsed-division.ts"));
  const inputs = {
    collateralAmount: "1501",
    collateralPrice: "100000000",
    debtAmount: "1000",
    debtPrice: "100000000",
    liquidationThresholdBps: "6667",
  };
  const scenario = { inputs };

  const expected = (await reference.run(scenario)) as { healthFactor: string; liquidatable: boolean };
  const actual = (await collapsed.run(scenario)) as { healthFactor: string; liquidatable: boolean };

  assert.equal(expected.healthFactor, "10000", "the contract lands exactly on the boundary");
  assert.equal(actual.healthFactor, "10007", "the collapsed form overshoots it");
  assert.equal(expected.liquidatable, false, "the contract says healthy");
  assert.notEqual(expected.healthFactor, actual.healthFactor);
});

test("a correct submission passes the same vectors the exploit fails", async () => {
  const good = await gateSubmission(submission({ implementation: "submissions/cached-health-factor.ts" }));
  const bad = await gateSubmission(submission({ implementation: "submissions/collapsed-division.ts" }));
  assert.equal(good.passed, true);
  assert.equal(bad.passed, false);
  assert.equal(good.vectors, bad.vectors, "the same vector set decides both");
});

test("the tolerance is maintainer-owned, so every course is exact", () => {
  for (const id of courseIds()) {
    const course = getCourse(id)!;
    assert.equal(course.tolerance, 0, `${id} must compare exactly, not within a loose tolerance`);
    assert.deepEqual(course.allow, [], `${id} must not allowlist any output path`);
  }
});

test("the reference and the vector file a course names both exist", () => {
  for (const id of courseIds()) {
    const course = getCourse(id)!;
    assert.ok(fs.existsSync(path.resolve(HERE, course.reference)), `${course.reference} is missing`);
    assert.ok(fs.existsSync(path.resolve(HERE, course.vectors)), `${course.vectors} is missing`);
    const vectors = JSON.parse(fs.readFileSync(path.resolve(HERE, course.vectors), "utf8")) as unknown;
    assert.ok(Array.isArray(vectors) && vectors.length > 0, `${course.vectors} must be a non-empty array`);
    for (const vector of vectors as { name: string; inputs: Record<string, unknown> }[]) {
      assert.equal(typeof vector.name, "string");
      assert.equal(typeof vector.inputs, "object");
      assert.ok(vector.name.length > 0, "every vector needs a name so a finding can point at it");
    }
  }
});

test("the vector sets cover the edge cases the courses claim to defend", () => {
  const read = (file: string) =>
    (JSON.parse(fs.readFileSync(path.resolve(HERE, file), "utf8")) as { name: string }[]).map((v) => v.name);

  const health = read(getCourse("lending::get_health_factor")!.vectors);
  assert.ok(health.includes("truncation-sensitive"), "the truncation case must be in the set");
  assert.ok(health.includes("no-oracle"), "the no-oracle zero must be in the set");
  assert.ok(health.includes("no-debt-sentinel"), "the sentinel must be in the set");
  assert.ok(health.includes("exactly-on-boundary"), "the boundary must be in the set");

  const sizing = read(getCourse("lending::liquidate")!.vectors);
  assert.ok(sizing.includes("healthy-cannot-be-liquidated"), "a healthy position must be in the set");
  assert.ok(sizing.includes("unknown-health-factor-cannot-be-liquidated"), "hf = 0 must be in the set");
  assert.ok(sizing.includes("minimum-close-factor-rounds"), "the 1 bps rounding must be in the set");

  const accrual = read(getCourse("lending::repay")!.vectors);
  assert.ok(accrual.includes("hourly-updates-drift"), "the reassociation case must be in the set");
  assert.ok(accrual.includes("small-principal-truncates"), "the truncation case must be in the set");
});

// ── The interest course: correctness and speed are independent ───────────────

test("a correct but deliberately slow strategy passes the gate", async () => {
  // Correctness and speed are separate axes: the gate judges only the first, and
  // the board is where the second shows up. If this failed, the board would be
  // inverted.
  const result = await gateSubmission(
    submission({ challenge: "lending::repay", implementation: "submissions/recompute-per-update.ts" }),
  );
  assert.equal(result.passed, true, result.detail);
  assert.equal(result.vectors, 8);
});

// ── The submission index ─────────────────────────────────────────────────────

const read = (file: string) => JSON.parse(fs.readFileSync(path.join(HERE, file), "utf8")) as Submission[];

const INDEX = read("submissions/index.json");
/** The negative fixture: entries the gate exists to reject. */
const INVALID = read("submissions/invalid.json");

test("the shipped submission index is well formed", () => {
  assert.ok(Array.isArray(INDEX) && INDEX.length >= 3);
  const ids = new Set<string>();
  for (const entry of INDEX) {
    for (const field of ["id", "author", "challenge", "implementation"] as const) {
      assert.equal(typeof entry[field], "string", `${entry.id} is missing ${field}`);
    }
    assert.equal(ids.has(entry.id), false, `${entry.id} is duplicated`);
    ids.add(entry.id);
    assert.ok(getCourse(entry.challenge) !== null, `${entry.id} targets no course`);
    assert.ok(fs.existsSync(path.resolve(HERE, entry.implementation)), `${entry.implementation} is missing`);
    assert.equal(entry.budget, undefined, `${entry.id} must not carry a budget`);
  }
});

test("every entry in the competition index passes the gate", async () => {
  // A contest index that ships a known-bad entry would make the default board
  // run permanently red, which trains people to ignore it. The negative case
  // lives in submissions/invalid.json instead.
  const gated = await gateAll(INDEX);
  const failed = gated.filter((g) => !g.gate.passed);
  assert.deepEqual(failed.map((g) => `${g.submission.id}: ${g.gate.reason}`), []);
});

test("the negative fixture is rejected for the right reason", async () => {
  // Without this, the negative test in CI would prove nothing.
  const gated = await gateAll(INVALID);
  assert.ok(gated.length >= 1, "the invalid fixture must not be empty");
  for (const { submission, gate } of gated) {
    assert.equal(gate.passed, false, `${submission.id} should not pass`);
    assert.equal(gate.reason, "diverged", `${submission.id} should fail on correctness, not on plumbing`);
  }
  assert.ok(gated.some((g) => g.submission.id === "collapsed-division"));
});

test("the reference entry is a legitimate self-score", async () => {
  const entry = INDEX.find((s) => s.id === "reference");
  assert.ok(entry, "the index needs a reference entry so the board has a known-zero baseline");
  const result = await gateSubmission(entry);
  assert.equal(result.passed, true);
});

test("gateAll keeps one outcome per submission, in order", async () => {
  const gated = await gateAll(INDEX);
  assert.deepEqual(gated.map((g) => g.submission.id), INDEX.map((s) => s.id));
  for (const { gate } of gated) {
    assert.equal(typeof gate.passed, "boolean");
    assert.ok(typeof gate.detail === "string" && gate.detail.length > 0);
  }
});
