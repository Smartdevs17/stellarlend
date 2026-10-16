/**
 * The correctness gate for gas golf submissions (#1014).
 *
 * A gas optimisation that changes behaviour is not an optimisation, so a
 * submission has to pass a differential comparison against the challenge's
 * reference implementation before it can be scored. That comparison is
 * `scripts/differential-test`'s — the runner #1067 built and left explicitly
 * unconnected in its README ("*the runner is the reusable shell. Connect it
 * to…*"). This module adds the two things competitive use requires and the
 * general-purpose runner deliberately does not have:
 *
 * 1. **The tolerance is maintainer-owned.** `differential-test`'s `--allow` list
 *    is supplied by whoever runs it, so `--allow '$'` would whitelist an entire
 *    output and make correctness a no-op. Here the tolerance and the allowed
 *    paths come from the challenge definition, which lives in this repository.
 *    A submission cannot relax either.
 * 2. **A submission may not carry its own budget.** `framework.rs`'s
 *    `get_budget()` returns `0` for an unknown operation and `budget == 0`
 *    means *unbudgeted*, so a submitter could post a brand-new operation with no
 *    budget and its score would be un-gatable. Budgets come from the course only.
 *
 * Storage read/write counts are deliberately **not** part of the gate: they are
 * hand-declared literals in the benchmark, so comparing them would be comparing
 * two pieces of documentation.
 */

import * as path from "node:path";
import { fileURLToPath } from "node:url";

import {
  loadImplementation,
  runDifferential,
  type Scenario,
} from "../differential-test/differential.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));

/** A golf course: one challenge, a reference implementation, and a fixed vector set. */
export interface Course {
  /** The `contract::fn` this course competes on. */
  challenge: string;
  title: string;
  /** What the challenge is, in one line. */
  summary: string;
  /** The trusted implementation. Submissions must agree with it. */
  reference: string;
  /** Vector file, relative to this tool's directory. */
  vectors: string;
  /**
   * Relative tolerance for numeric comparison. `0` means exact.
   *
   * Integer Soroban math rounds, so submissions are expected to return
   * pre-rounded integers; a non-zero tolerance here is a deliberate, visible
   * loosening, not a default.
   */
  tolerance: number;
  /**
   * Output paths a submission is allowed to differ on, e.g. a diagnostic field.
   * Empty for every course here: a gas optimisation has no business changing
   * any output.
   */
  allow: string[];
}

/** Why a submission failed the gate. */
export type GateReason =
  | "diverged"
  | "implementation-unreadable"
  | "vectors-unreadable"
  | "budget-override"
  | "unknown-challenge";

export interface GateFinding {
  path: string;
  kind: string;
  reference: unknown;
  submission: unknown;
}

export interface GateResult {
  challenge: string;
  passed: boolean;
  reason: GateReason | null;
  /** Vectors compared, and how many agreed. */
  vectors: number;
  divergences: GateFinding[];
  /** Human-readable explanation when `passed` is false. */
  detail: string;
}

const COURSES: Record<string, Course> = {
  "lending::get_health_factor": {
    challenge: "lending::get_health_factor",
    title: "Health factor computation",
    summary:
      "The contract computes (collateralValue * thresholdBps / 10000) * 10000 / debtValue as two truncating integer divisions. Collapsing them into one looks like a free saving and silently changes the answer wherever the first division truncates.",
    reference: "courses/health-factor.ts",
    vectors: "scenarios/health-factor.json",
    tolerance: 0,
    allow: [],
  },
  "lending::repay": {
    challenge: "lending::repay",
    title: "Interest accrual on repay",
    summary:
      "Accrue interest over a period before repaying. The benchmark suite already frames the algorithmic question as a full recompute (600k budget) against an incremental update (220k) and a same-block cache (80k); the course is that contest expressed so correctness is checkable.",
    reference: "courses/interest-accrual.ts",
    vectors: "scenarios/interest-accrual.json",
    tolerance: 0,
    allow: [],
  },
  "lending::liquidate": {
    challenge: "lending::liquidate",
    title: "Liquidation sizing",
    summary:
      "Size a liquidation: the maximum repayable amount under the close factor, and the collateral bonus under the incentive. Both are single truncating divisions with edge cases at zero, at the healthy boundary, and at the i128 ceiling.",
    reference: "courses/liquidation-sizing.ts",
    vectors: "scenarios/liquidation-sizing.json",
    tolerance: 0,
    allow: [],
  },
};

/** Course ids, sorted. */
export function courseIds(): string[] {
  return Object.keys(COURSES).sort();
}

/** Look a course up, or `null`. */
export function getCourse(challenge: string): Course | null {
  return COURSES[challenge] ?? null;
}

function resolveHere(relative: string): string {
  return path.resolve(HERE, relative);
}

async function readVectors(course: Course): Promise<Scenario[]> {
  const file = resolveHere(course.vectors);
  const parsed = JSON.parse(await import("node:fs").then((fs) => fs.readFileSync(file, "utf8"))) as unknown;
  if (!Array.isArray(parsed) || parsed.length === 0) {
    throw new Error(`${course.vectors} must be a non-empty JSON array of { name, inputs }`);
  }
  return parsed as Scenario[];
}

/** A submission manifest. Field-for-field the shape a contributor writes. */
export interface Submission {
  id: string;
  author: string;
  challenge: string;
  /** Path to the candidate implementation, relative to this tool. */
  implementation: string;
  notes?: string;
  /**
   * A benchmark report for this submission's branch. It is ingested and
   * integrity-checked; it is never trusted as a score on its own.
   */
  measurement?: string;
  /**
   * A submission must not carry a budget. Present here only so the gate can
   * reject it explicitly rather than ignore it.
   */
  budget?: number;
}

/** The outcome of gating one submission. */
export interface GatedSubmission {
  submission: Submission;
  gate: GateResult;
}

/**
 * Gate one submission against its challenge's reference.
 *
 * Exits the process with the gate's exit code semantics: `0` passed, `1`
 * diverged, `2` unusable. That matches `differential-test` so the two compose.
 */
export async function gateSubmission(submission: Submission): Promise<GateResult> {
  const base: GateResult = {
    challenge: submission.challenge,
    passed: false,
    reason: null,
    vectors: 0,
    divergences: [],
    detail: "",
  };

  const course = getCourse(submission.challenge);
  if (!course) {
    return { ...base, reason: "unknown-challenge", detail: `no course for "${submission.challenge}"` };
  }

  if (submission.budget !== undefined) {
    return {
      ...base,
      reason: "budget-override",
      detail:
        "a submission must not carry a budget: budgets are maintainer-owned, and an operation with no " +
        "budget is unbudgeted rather than un-gated",
    };
  }

  let reference: Awaited<ReturnType<typeof loadImplementation>>;
  let candidate: Awaited<ReturnType<typeof loadImplementation>>;
  let vectors: Scenario[];
  try {
    vectors = await readVectors(course);
  } catch (err) {
    return { ...base, reason: "vectors-unreadable", detail: (err as Error).message };
  }

  try {
    [reference, candidate] = await Promise.all([
      loadImplementation(resolveHere(course.reference)),
      loadImplementation(resolveHere(submission.implementation)),
    ]);
  } catch (err) {
    return { ...base, reason: "implementation-unreadable", detail: (err as Error).message };
  }

  const result = await runDifferential(vectors, reference, candidate, {
    // Maintainer-owned, from the course. The submission cannot widen this.
    tolerance: course.tolerance,
    allow: course.allow,
  });

  const divergences: GateFinding[] = result.findings.map((f) => ({
    path: f.path,
    kind: f.kind,
    reference: f.implA,
    submission: f.implB,
  }));

  if (result.ok) {
    return {
      ...base,
      passed: true,
      vectors: vectors.length,
      detail: `agrees with ${reference.name} on all ${vectors.length} vector(s)`,
    };
  }

  const first = divergences[0];
  return {
    ...base,
    reason: "diverged",
    vectors: vectors.length,
    divergences,
    detail:
      `diverges from ${reference.name} on ${divergences.length} of ${vectors.length} vector(s); ` +
      `first at ${first?.path} (${first?.kind}): ${JSON.stringify(first?.reference)} vs ${JSON.stringify(first?.submission)}`,
  };
}

/** Gate a list of submissions, keeping the outcome for each. */
export async function gateAll(submissions: Submission[]): Promise<GatedSubmission[]> {
  return Promise.all(
    submissions.map(async (submission) => ({ submission, gate: await gateSubmission(submission) })),
  );
}
