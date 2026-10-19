/**
 * Board rendering for the gas golf leaderboard (#1014).
 *
 * The most important thing this module does is render the **empty** state
 * honestly. `stellar-lend/benchmarks/baseline.json` has `"results": []` and
 * `gas-baseline.json` holds only `hello-world` measurements, so on the repository
 * as it stands there is no `lending::*` measurement to rank. A board that
 * printed a table anyway — with zeros, or with nothing — would be the exact
 * failure mode this tool exists to prevent, so the empty state says why.
 */

import { BLOCKING_REASONS, type IntegrityFinding } from "./measurements.ts";
import type { Leaderboard, ScoredEntry } from "./scoring.ts";
import { implausibleSaving } from "./scoring.ts";

function fmt(value: number, digits = 2): string {
  return value.toLocaleString("en-US", { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

/** A budget that may not exist. `0` is a real budget-less value; `null` is "unknown". */
function budgetLabel(budget: number | null): string {
  return budget === null ? "no budget" : `${budget.toLocaleString("en-US")} instructions`;
}

/** A figure that may not exist, rendered as a dash rather than as zero. */
function fmtOrDash(value: number | null, digits = 2): string {
  return value === null ? "—" : fmt(value, digits);
}

function pct(value: number, digits = 2): string {
  return `${value > 0 ? "+" : ""}${fmt(value, digits)}%`;
}

function bar(value: number, max: number, width = 20): string {
  if (max <= 0) return "";
  return "█".repeat(Math.max(1, Math.round((value / max) * width)));
}

export interface BoardContext {
  course: { sources: { publicFunctions: string; budgets: string } };
  /** Challenges with no budget at all, so they cannot be scored. */
  unscoredChallenges: string[];
  /** Integrity findings for the measurement set that was read. */
  integrity: IntegrityFinding[];
  /** Where the measurement came from, and its shape. */
  measurementSource: string;
  measurementShape: string;
  measurementTimestamp: string | null;
  submissionsConsidered: number;
}

/** Terminal board. */
export function renderText(board: Leaderboard, context: BoardContext): string {
  const lines: string[] = [];
  lines.push("Gas golf leaderboard");
  lines.push(
    `Targets from ${context.course.sources.publicFunctions} · budgets from ${context.course.sources.budgets}`,
  );
  lines.push(
    `Measurements from ${context.measurementSource} (${context.measurementShape}` +
      `${context.measurementTimestamp ? `, reported ${context.measurementTimestamp}` : ""})`,
  );
  lines.push("");

  const rankedChallenges = board.challenges.filter((c) => c.ranked.length > 0);
  if (rankedChallenges.length === 0) {
    lines.push("No rankable entries.");
    lines.push("");
    lines.push(...explainNoRanking(context, board));
    return lines.join("\n");
  }

  for (const challenge of rankedChallenges) {
    lines.push(`${challenge.challenge}  (budget ${budgetLabel(challenge.budget)})`);
    lines.push(
      "  Rank  Submission                        Author         Instructions  Utilisation   vs reference",
    );
    const max = Math.max(...challenge.ranked.map((e) => e.utilizationPct ?? 0));
    challenge.ranked.forEach((entry, index) => {
      const impl = implausibleSaving(entry.savingsPct);
      lines.push(
        [
          `  ${String(index + 1).padStart(4)}`,
          entry.submissionId.padEnd(32),
          entry.author.padEnd(14),
          (entry.instructions?.toLocaleString("en-US") ?? "—").padStart(12),
          `${fmtOrDash(entry.utilizationPct)}%`.padStart(12),
          `  ${bar(entry.utilizationPct ?? 0, max, 12)}`,
          implausible(entry),
          `  ${entry.savingsPct === null ? "—" : pct(entry.savingsPct)}`,
        ].join(" "),
      );
      if (impl) lines.push(`       ! ${impl}`);
    });
    lines.push("");
  }

  const notRanked = board.unscored;
  if (notRanked.length > 0) {
    lines.push(`Not ranked (${notRanked.length}):`);
    for (const entry of notRanked) {
      lines.push(`  ${entry.submissionId.padEnd(32)} ${entry.challenge.padEnd(34)} ${entry.disqualified}`);
    }
    lines.push("");
  }

  if (context.integrity.length > 0) {
    lines.push(`Measurement integrity (${context.integrity.length} finding(s)):`);
    for (const finding of context.integrity.slice(0, 12)) {
      lines.push(`  [${finding.reason}] ${finding.detail}`);
    }
    if (context.integrity.length > 12) {
      lines.push(`  … and ${context.integrity.length - 12} more`);
    }
    lines.push("");
  }

  return lines.join("\n");
}

function implausible(entry: ScoredEntry): string {
  return implausibleSaving(entry.savingsPct) === null ? "" : " [!]";
}

/** Why there is nothing to rank, in the reader's terms. */
export function explainNoRanking(context: BoardContext, board: Leaderboard): string[] {
  const lines: string[] = [];

  // Read from the same list the scorer uses, rather than restating it: a second
  // copy silently stops being the truth the moment a reason is added, and a
  // blocking finding that is not disclosed here is a finding nobody reads.
  const blocking = context.integrity.filter((f) => BLOCKING_REASONS.includes(f.reason));
  if (blocking.length > 0) {
    lines.push("Why there is nothing to rank:");
    // The reason label is shown here too, not just in the JSON: the markdown and
    // text renderers disagreeing about what to disclose is how a finding gets
    // read past.
    for (const finding of blocking.slice(0, 6)) lines.push(`  - [${finding.reason}] ${finding.detail}`);
    lines.push("");
  }

  if (board.unscored.length > 0) {
    const reasons = new Map<string, number>();
    for (const entry of board.unscored) {
      const key = entry.disqualified ?? "unknown";
      reasons.set(key, (reasons.get(key) ?? 0) + 1);
    }
    lines.push("Every entry was rejected, by reason:");
    for (const [reason, count] of [...reasons].sort()) lines.push(`  ${String(count).padStart(4)}  ${reason}`);
    lines.push("");
  }

  if (context.unscoredChallenges.length > 0) {
    lines.push(
      `${context.unscoredChallenges.length} challenge(s) have no budget in either budget block, so they are ` +
        "unscoreable: an operation with no budget is unbudgeted rather than un-gated.",
    );
    lines.push("");
  }

  if (context.submissionsConsidered === 0) {
    lines.push(
      "No submissions were provided. A gas golf entry is a correctness-gated implementation plus a " +
        "benchmark run for it; add one under submissions/ to appear here.",
    );
    lines.push("");
  }

  return lines;
}

/** Markdown board for a PR comment or job summary. */
export function renderMarkdown(board: Leaderboard, context: BoardContext): string {
  const out: string[] = [];
  out.push("## Gas golf leaderboard");
  out.push("");
  out.push(
    `Targets from \`${context.course.sources.publicFunctions}\`, budgets from ` +
      `\`${context.course.sources.budgets}\`, measurements from \`${context.measurementSource}\`.`,
  );
  out.push("");

  const rankedChallenges = board.challenges.filter((c) => c.ranked.length > 0);
  if (rankedChallenges.length === 0) {
    out.push("**No rankable entries.**");
    out.push("");
    for (const line of explainNoRanking(context, board)) out.push(line.trimEnd());
    return out.join("\n");
  }

  out.push(
    `Score is \`instructions / budget\` — the same \`utilizationPct\` the gas report uses. ` +
      `Lower is better, and the reference delta is only meaningful within a single benchmark session.`,
  );
  out.push("");
  for (const challenge of rankedChallenges) {
    out.push(`### \`${challenge.challenge}\` — budget ${budgetLabel(challenge.budget)}`);
    out.push("");
    out.push("| Rank | Submission | Author | Instructions | Utilisation | vs reference |");
    out.push("| ---: | --- | --- | ---: | ---: | ---: |");
    challenge.ranked.forEach((entry, index) => {
      out.push(
        `| ${index + 1} | \`${entry.submissionId}\` | ${entry.author} | ` +
          `${entry.instructions?.toLocaleString("en-US") ?? "—"} | ${fmtOrDash(entry.utilizationPct)}% | ` +
          `${entry.savingsPct === null ? "—" : pct(entry.savingsPct)} |`,
      );
    });
    out.push("");
    for (const entry of challenge.ranked) {
      const impl = implausibleSaving(entry.savingsPct);
      if (impl) out.push(`> \`${entry.submissionId}\`: ${impl}`);
    }
  }

  if (board.unscored.length > 0) {
    out.push("### Not ranked");
    out.push("");
    out.push("| Submission | Challenge | Reason |");
    out.push("| --- | --- | --- |");
    for (const entry of board.unscored) {
      out.push(`| \`${entry.submissionId}\` | \`${entry.challenge}\` | ${entry.disqualified} |`);
    }
    out.push("");
  }

  if (context.integrity.length > 0) {
    out.push("### Measurement integrity");
    out.push("");
    for (const finding of context.integrity.slice(0, 20)) {
      out.push(`- **${finding.reason}** — ${finding.detail}`);
    }
    out.push("");
  }

  return out.join("\n");
}

/** JSON shape, with the numbers kept as numbers so the board is scriptable. */
export function toJson(board: Leaderboard, context: BoardContext): unknown {
  return {
    sources: {
      publicFunctions: context.course.sources.publicFunctions,
      budgets: context.course.sources.budgets,
      measurement: context.measurementSource,
      measurementShape: context.measurementShape,
      measurementTimestamp: context.measurementTimestamp,
    },
    scoring: {
      metric: "utilizationPct",
      formula: "instructions / budget * 100",
      direction: "lower is better",
      tiebreak: "memory bytes, then submission id",
      note: "only a same-session reference delta is meaningful; storage counts are declared, not measured, and never rank",
    },
    totals: board.totals,
    challenges: board.challenges.map((challenge) => ({
      challenge: challenge.challenge,
      budget: challenge.budget,
      referenceInstructions: challenge.referenceInstructions,
      ranked: challenge.ranked,
      entries: challenge.entries,
    })),
    unscored: board.unscored,
    integrity: context.integrity,
    unscoredChallenges: context.unscoredChallenges,
  };
}
