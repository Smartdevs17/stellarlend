#!/usr/bin/env node
/**
 * Gas golf competition leaderboard and optimization harness (#1014).
 *
 * Gates a submission on correctness, then ranks it on measured gas. The two are
 * deliberately separate: the gate is deterministic and cheap, the score comes
 * from the Rust benchmark suite because Soroban gas cannot be measured from
 * JavaScript.
 *
 * Run (no install needed on Node >= 22):
 *   node --experimental-strip-types scripts/gas-golf/index.ts --list-challenges
 *   node --experimental-strip-types scripts/gas-golf/index.ts --list-courses
 *   node --experimental-strip-types scripts/gas-golf/index.ts --gate all
 *   node --experimental-strip-types scripts/gas-golf/index.ts --submissions submissions/index.json
 *   node --experimental-strip-types scripts/gas-golf/index.ts --format markdown --out board.md
 *
 * Exit codes: 0 ok, 1 gate failed or a ranking rule rejected something, 2 usage error.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import { renderMarkdown, renderText, toJson, type BoardContext } from "./board.ts";
import { findChallenge, loadCourseFromDisk, OPERATION_TYPES, type Course } from "./challenges.ts";
import { courseIds, gateSubmission, getCourse, type Submission } from "./gate.ts";
import {
  checkIntegrity,
  hasBlockingFinding,
  indexById,
  loadMeasurements,
  parseMeasurements,
  type Measurement,
  type MeasurementSet,
} from "./measurements.ts";
import { buildLeaderboard, type ScoreInput } from "./scoring.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");

const DEFAULT_SUBMISSIONS = "submissions/index.json";
/** The manifest id that self-scores. See `scoring.ts` rule 2. */
const REFERENCE_ID = "reference";
const DEFAULT_REFERENCE_MEASUREMENTS = "stellar-lend/benchmark-results.json";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

function flag(name: string): boolean {
  return process.argv.includes(`--${name}`);
}

const USAGE = `Gas golf leaderboard and optimization harness (#1014)

Usage:
  index.ts [--submissions <index.json>] [options]
  index.ts --list-challenges
  index.ts --list-courses
  index.ts --gate all

Options:
  --submissions <file>   Submission index (default: ${DEFAULT_SUBMISSIONS}).
                         Relative to this tool, or to the repository root.
  --out <file>           Same resolution as --submissions.
  --reference <file>     Benchmark report for the reference, same session
                         (default: ${DEFAULT_REFERENCE_MEASUREMENTS} when present)
  --gate all|<id>        Only run the correctness gate (default: off)
  --list-challenges      Print the open targets and exit
  --list-courses         Print the gated courses and exit
  --format <fmt>         text | json | markdown (default: text)
  --out <file>           Write the report to a file instead of stdout
  --require-fresh        Refuse measurements that cannot be shown to come from
                          one session: rows from several commits, or rows that
                          name no commit at all
  --help                 Print this help and exit

Exit codes:
  0 ok · 1 gate failed · 2 usage error
`;

/**
 * Resolve a path from a flag or a manifest.
 *
 * Manifest paths are relative to this tool (so `submissions/x.ts` is stable as
 * the tool moves), but a person running the CLI from the repository root will
 * naturally type `scripts/gas-golf/submissions/x.ts`. Both work.
 */
function resolveHere(relative: string): string {
  if (path.isAbsolute(relative)) return relative;
  const local = path.resolve(HERE, relative);
  if (fs.existsSync(local)) return local;
  return path.resolve(REPO_ROOT, relative);
}

function resolveRepo(relative: string): string {
  return path.isAbsolute(relative) ? relative : path.join(REPO_ROOT, relative);
}

/** Read the submission index, validating each manifest's shape. */
function loadSubmissions(file: string): Submission[] {
  const text = fs.readFileSync(resolveHere(file), "utf8");
  const parsed = JSON.parse(text) as unknown;
  if (!Array.isArray(parsed)) {
    throw new Error(`${file} must be a JSON array of submission manifests`);
  }
  return parsed.map((raw, index) => {
    if (!raw || typeof raw !== "object") {
      throw new Error(`${file}[${index}] must be an object`);
    }
    const entry = raw as Record<string, unknown>;
    for (const field of ["id", "author", "challenge", "implementation"] as const) {
      if (typeof entry[field] !== "string" || (entry[field] as string).length === 0) {
        throw new Error(`${file}[${index}] is missing a "${field}"`);
      }
    }
    return {
      id: entry.id as string,
      author: entry.author as string,
      challenge: entry.challenge as string,
      implementation: entry.implementation as string,
      ...(typeof entry.notes === "string" ? { notes: entry.notes } : {}),
      ...(typeof entry.measurement === "string" ? { measurement: entry.measurement } : {}),
      // Carried so the gate can reject a budget-carrying submission explicitly.
      ...(typeof entry.budget === "number" ? { budget: entry.budget } : {}),
    } as Submission;
  });
}

/** Load a submission's measurement file, or `null` when it supplied none. */
function loadSubmissionMeasurement(submission: Submission): {
  instructions: number | null;
  memoryBytes: number | null;
  note: string;
} {
  if (!submission.measurement) {
    return { instructions: null, memoryBytes: null, note: "no measurement supplied" };
  }
  let set: MeasurementSet;
  try {
    set = parseMeasurements(fs.readFileSync(resolveHere(submission.measurement), "utf8"));
  } catch (err) {
    return { instructions: null, memoryBytes: null, note: `unreadable: ${(err as Error).message}` };
  }
  const findings = checkIntegrity(set, [{ id: submission.challenge }]);
  if (hasBlockingFinding(findings)) {
    return {
      instructions: null,
      memoryBytes: null,
      note: `measurement rejected: ${findings[0]?.reason}`,
    };
  }
  const index = indexById(set);
  const fn = submission.challenge.split("::").slice(1).join("::");
  const row: Measurement | undefined =
    index.get(submission.challenge) ?? [...index.values()].find((m) => m.fn === fn);
  if (!row) {
    return { instructions: null, memoryBytes: null, note: "measurement has no row for this challenge" };
  }
  return { instructions: row.instructions, memoryBytes: row.memoryBytes, note: "" };
}

function findReferenceMeasurement(
  challenge: string,
  sets: { set: MeasurementSet; label: string }[],
): number | null {
  const fn = challenge.split("::")[1];
  for (const { set } of sets) {
    const index = indexById(set);
    const row = index.get(challenge) ?? [...index.values()].find((m) => m.fn === fn);
    if (row && row.instructions > 0) return row.instructions;
  }
  return null;
}

async function main(): Promise<void> {
  if (flag("help") || flag("h")) {
    console.log(USAGE);
    process.exit(0);
  }

  const course: Course = loadCourseFromDisk(REPO_ROOT);
  const format = (arg("format") ?? "text") as "text" | "json" | "markdown";
  if (format !== "text" && format !== "json" && format !== "markdown") {
    console.error(`error: --format must be text|json|markdown, got "${format}"`);
    process.exit(2);
  }

  if (flag("list-challenges")) {
    for (const challenge of course.challenges) {
      const budget = challenge.budget === null ? "no budget" : `${challenge.budget}`;
      console.log(
        `${challenge.id.padEnd(52)} ${challenge.operationType.padEnd(12)} ${budget.padStart(11)}  ` +
          `(${challenge.budgetSource})`,
      );
    }
    console.log(`\n${course.challenges.length} target(s) · ${course.unscored.length} without a budget`);
    process.exit(0);
  }

  if (flag("list-courses")) {
    for (const id of courseIds()) {
      const golf = getCourse(id)!;
      console.log(`${golf.challenge}\n  ${golf.title}\n  ${golf.summary}\n`);
    }
    process.exit(0);
  }

  const gateOnly = arg("gate");

  // `--gate all` with no `--submissions` still has something to gate: the
  // shipped index.
  const submissionsFile = arg("submissions") ?? DEFAULT_SUBMISSIONS;
  let submissions: Submission[] = [];
  if (submissionsFile) {
    const resolved = resolveHere(submissionsFile);
    if (!fs.existsSync(resolved)) {
      if (flag("gate")) {
        console.error(`error: --submissions file not found: ${submissionsFile}`);
        process.exit(2);
      }
      console.error(`error: no submissions index at ${submissionsFile}`);
      process.exit(2);
    }
    submissions = loadSubmissions(resolved);
  }

  if (gateOnly) {
    const targets = gateOnly === "all" ? submissions : submissions.filter((s) => s.id === gateOnly);
    if (targets.length === 0) {
      console.error(`error: --gate ${gateOnly} matched no submission`);
      process.exit(2);
    }
    let failed = 0;
    for (const submission of targets) {
      const result = await gateSubmission(submission);
      console.log(`${submission.id.padEnd(32)} ${result.passed ? "PASS" : "REJECT"}  ${result.detail}`);
      if (!result.passed) failed++;
    }
    process.exit(failed > 0 ? 1 : 0);
  }

  // Measurements: the reference is whatever the repository currently holds, and
  // the board reports honestly when there is nothing usable in it.
  const referencePath = arg("reference") ?? DEFAULT_REFERENCE_MEASUREMENTS;
  const referenceAbsolute = resolveRepo(referencePath);
  const referenceSets: { set: MeasurementSet; label: string }[] = [];
  let integrityContext = { source: "none", shape: "none", timestamp: null as string | null, findings: [] as ReturnType<typeof checkIntegrity> };

  if (fs.existsSync(referenceAbsolute)) {
    try {
      const set = loadMeasurements(referenceAbsolute);
      referenceSets.push({ set, label: referencePath });
      const findings = checkIntegrity(set, course.challenges, { requireFresh: flag("require-fresh") });
      integrityContext = {
        source: referencePath,
        shape: set.shape,
        timestamp: set.reportedAt,
        findings,
      };
    } catch (err) {
      integrityContext = {
        source: referencePath,
        shape: "unreadable",
        timestamp: null,
        findings: [
          {
            reason: "unreadable",
            id: "",
            detail: `${referencePath}: ${(err as Error).message}`,
          },
        ],
      };
    }
  } else {
    integrityContext = {
      source: `${referencePath} (absent)`,
      shape: "none",
      timestamp: null,
      findings: [
        {
          reason: "empty",
          id: "",
          detail:
            `${referencePath} does not exist. Run ./run-benchmarks.sh to produce a report; ` +
            "without one there is nothing to score against.",
        },
      ],
    };
  }

  // Gate every submission first: correctness is a precondition for ranking.
  const gated = await Promise.all(
    submissions.map(async (submission) => ({ submission, gate: await gateSubmission(submission) })),
  );

  const scoreInputs: ScoreInput[] = gated.map(({ submission, gate }) => {
    const challenge = findChallenge(course, submission.challenge);
    const measurement = loadSubmissionMeasurement(submission);

    // Only a reference report establishes a session. The `reference` entry
    // self-scores so the board has a known-zero baseline, but it is never used
    // as the yardstick for anyone else: pairing two contestants across
    // different builds is exactly the meaningless absolute number rule 2 of
    // scoring.ts exists to prevent.
    const referenceInstructions =
      submission.id === REFERENCE_ID ? measurement.instructions : findReferenceMeasurement(submission.challenge, referenceSets);

    return {
      submissionId: submission.id,
      author: submission.author,
      challenge: submission.challenge,
      notes: submission.notes ?? "",
      gatePassed: gate.passed,
      instructions: measurement.instructions,
      memoryBytes: measurement.memoryBytes,
      referenceInstructions,
      budget: challenge.budget,
    };
  });

  // The reference is per-challenge and per-session, so it is collected once
  // rather than inferred from whichever entry happens to be ranked first.
  const references: Record<string, number> = {};
  for (const { submission } of gated) {
    if (references[submission.challenge] !== undefined) continue;
    const found = findReferenceMeasurement(submission.challenge, referenceSets);
    if (found !== null) references[submission.challenge] = found;
  }

  const board = buildLeaderboard(scoreInputs, references);

  const context: BoardContext = {
    course: { sources: course.sources },
    unscoredChallenges: course.unscored,
    integrity: integrityContext.findings,
    measurementSource: integrityContext.source,
    measurementShape: integrityContext.shape,
    measurementTimestamp: integrityContext.timestamp,
    submissionsConsidered: submissions.length,
  };

  const body =
    format === "json"
      ? JSON.stringify(toJson(board, context), null, 2)
      : format === "markdown"
        ? renderMarkdown(board, context)
        : renderText(board, context);

  const out = arg("out");
  if (out) {
    const outPath = resolveHere(out);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, `${body}\n`);
    const relative = path.relative(REPO_ROOT, outPath);
    // `path.relative` walks out of the repository with `..`, which reads like a
    // mistake in a CI log.
    console.error(
      `Wrote ${format} board to ${relative.startsWith("..") ? outPath : relative}`,
    );
  } else {
    console.log(body);
  }

  const gateFailures = gated.filter(({ gate }) => !gate.passed);
  if (gateFailures.length > 0) {
    console.error(
      `\n::error::${gateFailures.length} submission(s) failed the correctness gate: ` +
        gateFailures.map(({ submission, gate }) => `${submission.id} (${gate.reason})`).join(", "),
    );
    process.exit(1);
  }

  console.error(
    `\nScoring: ${board.totals.rankedEntries} ranked, ${board.totals.disqualifiedEntries} not ranked. ` +
      `Operation types: ${OPERATION_TYPES.join(", ")}.`,
  );
  process.exit(0);
}

main().catch((err) => {
  console.error(`error: ${(err as Error).message}`);
  process.exit(2);
});
