/**
 * Scoring and ranking for the gas golf leaderboard (#1014).
 *
 * The score is `utilizationPct = instructions / budget` — lower is better. That
 * primitive already exists in the repository as `FunctionRow.utilizationPct`
 * (`api/src/services/gasReport/report.ts:191`) and is already served at
 * `/api/analytics/gas/contract/budgets`, so using it means the leaderboard and
 * the gas report can never disagree about what "over budget" means. It is also
 * the only honest way to rank across challenges: a `deposit` with an 800k budget
 * and a `flash_loan` with 1.8M are not comparable as raw instruction counts.
 *
 * Four rules keep the ranking from being a fiction:
 *
 * 1. **A failed correctness gate disqualifies**, whatever the measurement says.
 *    Speed without correctness is not an optimisation, it is a bug.
 * 2. **A score is always paired with a same-session reference.** Instruction
 *    counts are toolchain- and build-dependent, so an absolute number is
 *    meaningless; only the reference↔submission delta within one run carries
 *    information. `gas-regression.yml:36-40` makes the same argument about cold
 *    caches: "# No rust cache on purpose — a cold, reproducible build keeps the
 *    measurement comparable with how the committed baseline was produced."
 * 3. **A saving larger than the reference itself is implausible** and is flagged
 *    rather than celebrated: you cannot do work in negative instructions, so
 *    `> 100%` savings means the measurement is wrong, not that the entry is good.
 * 4. **Only measured inputs rank.** `instructions` and `memory_bytes` come from
 *    `env.cost_estimate()`. `storage_reads` / `storage_writes` are hand-declared
 *    literals in every `BenchmarkResult::new(...)` call and are never a ranking
 *    input.
 */

/** A contestant's entry for one challenge. */
export interface ScoredEntry {
  submissionId: string;
  author: string;
  challenge: string;
  budget: number;
  /** `null` when the entry supplied no readable measurement. */
  instructions: number | null;
  memoryBytes: number | null;
  /** `instructions / budget * 100`. Lower is better. `null` when unscoreable. */
  utilizationPct: number | null;
  /** Instructions for the reference, in the same session. */
  referenceInstructions: number | null;
  /** `100 * (reference - submission) / reference`. Negative means a regression. */
  savingsPct: number | null;
  /** Whether the correctness gate passed. */
  gatePassed: boolean;
  /** Why the entry cannot be ranked, when it cannot. */
  disqualified: Disqualification | null;
  notes: string;
}

export type Disqualification =
  | "gate-failed"
  | "no-measurement"
  | "no-reference"
  | "unreadable-measurement"
  | "no-budget";

/** A ranked challenge. */
export interface ChallengeBoard {
  challenge: string;
  /** `null` when no budget block covers this operation, so it cannot be scored. */
  budget: number | null;
  entries: ScoredEntry[];
  /** The ranked, scorable entries, best first. */
  ranked: ScoredEntry[];
  best: ScoredEntry | null;
  referenceInstructions: number | null;
}

/** The whole board. */
export interface Leaderboard {
  challenges: ChallengeBoard[];
  /** Submissions that were not ranked, and why. */
  unscored: ScoredEntry[];
  totals: {
    challenges: number;
    rankedEntries: number;
    disqualifiedEntries: number;
    bestOverall: { submissionId: string; challenge: string; savingsPct: number } | null;
  };
}

export interface ScoreInput {
  submissionId: string;
  author: string;
  challenge: string;
  notes?: string;
  gatePassed: boolean;
  /** `null` when the submission supplied no readable measurement. */
  instructions: number | null;
  memoryBytes: number | null;
  /** The reference's measurement for the same challenge, same session. */
  referenceInstructions: number | null;
  /** From the course. A `null` budget makes the entry unscoreable. */
  budget: number | null;
}

/** `instructions / budget` as a percentage, or `null` when unscoreable. */
export function utilizationPct(instructions: number, budget: number): number | null {
  if (budget <= 0 || instructions <= 0) return null;
  return (instructions / budget) * 100;
}

/**
 * Percentage saved against the reference. `null` without a reference — an
 * absolute number is not a score.
 */
export function savingsPct(instructions: number, referenceInstructions: number | null): number | null {
  if (referenceInstructions === null || referenceInstructions <= 0) return null;
  return ((referenceInstructions - instructions) / referenceInstructions) * 100;
}

/**
 * A saving beyond 100% is not a great optimisation, it is a broken measurement.
 * Returns the reason, or `null` when the figure is plausible.
 */
export function implausibleSaving(savings: number | null): string | null {
  if (savings === null) return null;
  if (savings > 100) {
    return (
      `a ${savings.toFixed(2)}% saving is impossible: the submission cannot use fewer than zero ` +
      "instructions, so the measurement is not trustworthy"
    );
  }
  if (savings < -100) {
    return `a ${savings.toFixed(2)}% regression implies a negative instruction count`;
  }
  return null;
}

function disqualify(input: ScoreInput): Disqualification | null {
  if (!input.gatePassed) return "gate-failed";
  if (input.budget === null) return "no-budget";
  if (input.instructions === null) return input.referenceInstructions === null ? "no-measurement" : "no-measurement";
  if (input.instructions <= 0) return "unreadable-measurement";
  if (input.referenceInstructions === null) return "no-reference";
  return null;
}

function scoreEntry(input: ScoreInput): ScoredEntry {
  const reason = disqualify(input);
  const instructions = input.instructions;
  const memoryBytes = input.memoryBytes;
  const utilization = input.instructions === null ? null : utilizationPct(input.instructions, input.budget ?? 0);
  const savings = input.instructions === null ? null : savingsPct(input.instructions, input.referenceInstructions);

  return {
    submissionId: input.submissionId,
    author: input.author,
    challenge: input.challenge,
    budget: input.budget ?? 0,
    instructions,
    memoryBytes,
    // Coerced to zero these would be indistinguishable from a real result: a
    // missing reference would read as "0% saved", which is a perfect score.
    utilizationPct: utilization,
    referenceInstructions: input.referenceInstructions,
    savingsPct: savings,
    gatePassed: input.gatePassed,
    disqualified: reason,
    notes: input.notes ?? "",
  };
}

/**
 * Rank entries within a challenge.
 *
 * Sorted by `utilizationPct` ascending — the repo's own primitive, so ties and
 * ordering mean the same thing here as in the gas report. Entries that could not
 * be ranked sort last, and are listed in `unscored` rather than on the board.
 */
export function rankChallenge(
  challenge: string,
  budget: number | null,
  entries: ScoreInput[],
  /**
   * The reference's measurement for this challenge, from the same benchmark
   * session. Passed in rather than inferred from the entries: it describes the
   * run, not any one contestant, and inferring it would let the first-ranked
   * submission masquerade as the thing it is being compared against.
   */
  referenceInstructions: number | null = null,
): ChallengeBoard {
  const scored = entries.map(scoreEntry);
  const rankable = scored.filter((e) => e.disqualified === null);
  // `rankable` entries always have a budget, a measurement and a reference, so
  // these are non-null here; the fallbacks are only to keep the comparator total.
  const ranked = [...rankable].sort(
    (a, b) =>
      (a.utilizationPct ?? 0) - (b.utilizationPct ?? 0) ||
      (a.memoryBytes ?? 0) - (b.memoryBytes ?? 0) ||
      a.submissionId.localeCompare(b.submissionId),
  );

  return {
    challenge,
    budget,
    entries: scored,
    ranked,
    best: ranked[0] ?? null,
    referenceInstructions,
  };
}

/** Build the whole board. */
export function buildLeaderboard(
  inputs: ScoreInput[],
  /** Per-challenge reference measurement, from the same session. */
  references: Record<string, number> = {},
): Leaderboard {
  const byChallenge = new Map<string, ScoreInput[]>();
  for (const input of inputs) {
    const list = byChallenge.get(input.challenge) ?? [];
    list.push(input);
    byChallenge.set(input.challenge, list);
  }
  const referencesByChallenge = new Map(
    Object.entries(references).filter(([, value]) => typeof value === "number" && value > 0),
  );

  const challenges = [...byChallenge.entries()]
    .map(([challenge, entries]) =>
      rankChallenge(challenge, entries[0]?.budget ?? null, entries, referencesByChallenge.get(challenge) ?? null),
    )
    .sort((a, b) => a.challenge.localeCompare(b.challenge));

  const unscored = challenges.flatMap((c) => c.entries).filter((e) => e.disqualified !== null);

  const rankedEntries = challenges.flatMap((c) => c.ranked);
  const bestOverall = rankedEntries
    .filter((e) => e.savingsPct !== null)
    .sort((a, b) => (b.savingsPct ?? 0) - (a.savingsPct ?? 0))[0];

  return {
    challenges,
    unscored,
    totals: {
      challenges: challenges.length,
      rankedEntries: rankedEntries.length,
      disqualifiedEntries: unscored.length,
      bestOverall: bestOverall
        ? {
            submissionId: bestOverall.submissionId,
            challenge: bestOverall.challenge,
            savingsPct: bestOverall.savingsPct ?? 0,
          }
        : null,
    },
  };
}
