/**
 * Report rendering for the position health simulator (#1013).
 *
 * Three formats, one data shape: a terminal table, a markdown block for a PR
 * comment or job summary, and JSON for anything programmatic. The markdown and
 * text renderers share the same numbers, so a reviewer reading the comment sees
 * what CI computed.
 */

import { healthFactorToNumber, type SimulationReport } from "./simulation.ts";

/** Health factors are bps; show them as a ratio to two decimals. */
function hf(healthFactor: bigint): string {
  return healthFactorToNumber(healthFactor).toFixed(4);
}

function pct(value: number | null, digits = 2): string {
  return value === null ? "—" : `${value.toFixed(digits)}%`;
}

function bar(value: number, max: number, width = 22): string {
  if (max <= 0 || !Number.isFinite(value)) return "";
  return "█".repeat(Math.max(1, Math.round((value / max) * width)));
}

/** Describe a health factor's distance from the 1.0 boundary on a log-ish scale. */
function severityBar(healthFactor: bigint): string {
  const ratio = healthFactorToNumber(healthFactor);
  if (!Number.isFinite(ratio) || ratio <= 0) return "";
  // Full bar at 2.0x, empty at 1.0x, so the interesting region gets the width.
  const fill = Math.max(0, Math.min(1, (ratio - 1) / 1));
  return `${"█".repeat(Math.round(fill * 20))}${"·".repeat(20 - Math.round(fill * 20))}`;
}

function healthLine(report: SimulationReport): string[] {
  const { baseline, baselineRisk, distance, breakEven: be } = report;
  const lines: string[] = [];
  lines.push(`Position: ${report.positionName}`);
  lines.push(
    `Health factor: ${hf(baseline.healthFactor)} (${baselineRisk})  ${severityBar(baseline.healthFactor)}`,
  );
  lines.push(
    `Collateral value ${baseline.collateralValue} · debt value ${baseline.debtValue} · ` +
      `threshold ${baseline.liquidationThresholdBps} bps`,
  );
  if (baseline.healthFactorKnown) {
    lines.push(
      `Distance to liquidation: ${pct(distance.dropPercent)} collateral price fall ` +
        `(break-even multiplier ${be.priceMultiplier === null ? "—" : be.priceMultiplier.toFixed(4)})`,
    );
    if (be.recoveryTo15x !== null) {
      lines.push(
        `Recovery to 1.5x: ${pct(be.recoveryTo15x * 100, 2)} collateral price rise`,
      );
    }
    lines.push(
      `Liquidatable now: ${baseline.isLiquidatable ? "yes" : "no"}` +
        (baseline.isLiquidatable
          ? ` · max closeable ${baseline.maxLiquidatableAmount} (close factor ${baseline.closeFactorBps} bps)`
          : ""),
    );
  } else {
    lines.push(`Health factor unknown: ${baseline.unknownReason}`);
    lines.push("The contract reports 0 here, and get_max_liquidatable_amount returns 0, so no liquidation can proceed.");
  }
  return lines;
}

function scenarioTable(report: SimulationReport): string[] {
  const lines: string[] = [];
  lines.push("");
  lines.push("Historical scenarios (scenarios/*.json):");
  lines.push("Scenario                       HF        Risk          ΔHF        Max closeable");
  for (const { scenario, step, unmatched } of report.scenarios) {
    lines.push(
      [
        scenario.name.padEnd(30),
        hf(step.health.healthFactor).padStart(8),
        step.risk.padEnd(13),
        (step.health.healthFactor === report.baseline.healthFactor
          ? "—"
          : `${(Number(step.healthFactorDelta) / 10_000 * 100).toFixed(2)}%`
        ).padStart(9),
        step.health.maxLiquidatableAmount.toString().padStart(16),
      ].join(" "),
    );
    if (unmatched.length > 0) {
      lines.push(`${" ".repeat(30)}  (scenario also moves ${unmatched.join(", ")}, which this position does not hold)`);
    }
  }
  return lines;
}

function gridTable(report: SimulationReport): string[] {
  const lines: string[] = [];
  lines.push("");
  lines.push(`Price grid — ${report.grid.grid.name}: ${report.grid.grid.description}`);
  lines.push("Collateral move   HF        Risk           ΔHF");
  const maxDelta = Math.max(
    1,
    ...report.grid.steps.map((s) => Math.abs(Number(s.healthFactorDelta))),
  );
  for (const step of report.grid.steps) {
    const deltaPct = (Number(step.healthFactorDelta) / 10_000) * 100;
    lines.push(
      [
        step.label.padStart(15),
        hf(step.health.healthFactor).padStart(8),
        step.risk.padEnd(14),
        (deltaPct === 0 ? "—" : `${deltaPct > 0 ? "+" : ""}${deltaPct.toFixed(2)}%`).padStart(8),
        `  ${bar(Math.abs(Number(step.healthFactorDelta)), maxDelta, 12)}`,
        step.becameLiquidatable ? "  <- liquidates here" : "",
      ].join(" "),
    );
  }
  return lines;
}

/**
 * Heatmap of the shock grid: rows are collateral moves, columns are debt moves.
 * Each cell shows the resulting health factor and a fill glyph so the shape of
 * the risk surface is readable at a glance in a terminal or a markdown comment.
 */
function heatmapCells(report: SimulationReport): {
  rows: string[];
  cols: string[];
  cells: Map<string, { healthFactor: bigint; risk: string; liquidatable: boolean }>;
} {
  const rows = report.heatmap.rowLabels;
  const cols = report.heatmap.colLabels;
  const cells = new Map<
    string,
    { healthFactor: bigint; risk: string; liquidatable: boolean }
  >();
  for (const cell of report.heatmap.cells) {
    cells.set(`${cell.rowIndex}:${cell.colIndex}`, {
      healthFactor: cell.healthFactor,
      risk: cell.risk,
      liquidatable: cell.isLiquidatable,
    });
  }
  return { rows, cols, cells };
}

/** Single-glyph severity marker for a heatmap cell. */
function heatGlyph(healthFactor: bigint): string {
  const ratio = healthFactorToNumber(healthFactor);
  if (!Number.isFinite(ratio) || ratio <= 0) return "?";
  if (ratio < 1) return "X";
  if (ratio < 1.1) return "!";
  if (ratio < 1.5) return "~";
  if (ratio < 2) return "=";
  return "#";
}

function heatmapTable(report: SimulationReport): string[] {
  const { rows, cols, cells } = heatmapCells(report);
  const lines: string[] = [];
  lines.push("");
  lines.push(`Shock heatmap — ${report.heatmap.name}: ${report.heatmap.description}`);
  lines.push("Rows: collateral price move · Columns: debt price move");
  lines.push(`Legend: X liquidatable · ! <1.1x · ~ <1.5x · = <2.0x · # >=2.0x · ? unknown`);
  const colWidth = 9;
  const header = ["".padEnd(12), ...cols.map((c) => c.padStart(colWidth))].join(" ");
  lines.push(header);
  for (let r = 0; r < rows.length; r++) {
    const parts: string[] = [rows[r].padStart(12)];
    for (let c = 0; c < cols.length; c++) {
      const cell = cells.get(`${r}:${c}`);
      if (!cell) {
        parts.push("—".padStart(colWidth));
        continue;
      }
      const label = `${heatGlyph(cell.healthFactor)} ${hf(cell.healthFactor)}`;
      parts.push(label.padStart(colWidth));
    }
    lines.push(parts.join(" "));
  }
  return lines;
}

function thresholdTable(report: SimulationReport): string[] {
======
  const lines: string[] = [];
  lines.push("");
  lines.push(
    `Liquidation threshold sweep (admin-settable 1..10000 bps; contract default ${report.thresholds.defaultBps}):`,
  );
  lines.push("Threshold bps   HF        Risk           ΔHF vs default");
  for (const step of report.thresholds.steps) {
    const isDefault = step.thresholdBps === report.thresholds.defaultBps;
    const deltaPct = (Number(step.healthFactorDelta) / 10_000) * 100;
    lines.push(
      [
        step.thresholdBps.toString().padStart(13),
        hf(step.healthFactor).padStart(8),
        step.risk.padEnd(14),
        (deltaPct === 0 ? "—" : `${deltaPct > 0 ? "+" : ""}${deltaPct.toFixed(2)}%`).padStart(16),
        isDefault ? "  (contract default)" : "",
        step.isLiquidatable ? "  <- liquidatable" : "",
      ].join(" "),
    );
  }
  return lines;
}

/** Terminal report. */
export function renderText(report: SimulationReport): string {
  const lines: string[] = ["Position health simulation", ""];
  lines.push(...healthLine(report));
  if (report.scenarios.length > 0) lines.push(...scenarioTable(report));
  lines.push(...gridTable(report));
  lines.push(...heatmapTable(report));
  lines.push(...thresholdTable(report));
  lines.push("");
  lines.push(
    `Health factor math ported from stellar-lend/contracts/lending/src/views.rs; ` +
      `constants read from ${report.constants.sources.views} and ${report.constants.sources.borrow}.`,
  );
  return lines.join("\n");
}

/** Markdown report for a PR comment or job summary. */
export function renderMarkdown(report: SimulationReport): string {
  const { baseline, baselineRisk, distance, breakEven: be } = report;
  const out: string[] = [];
  out.push(`## Position health simulation — ${report.positionName}`);
  out.push("");
  out.push(
    `**Health factor ${hf(baseline.healthFactor)}** (${baselineRisk}) · ` +
      `collateral value ${baseline.collateralValue} · debt value ${baseline.debtValue} · ` +
      `liquidation threshold ${baseline.liquidationThresholdBps} bps`,
  );
  out.push("");
  if (baseline.healthFactorKnown) {
    out.push(
      `- Distance to liquidation: **${pct(distance.dropPercent)}** collateral price fall ` +
        `(break-even multiplier ${be.priceMultiplier === null ? "—" : be.priceMultiplier.toFixed(4)})`,
    );
    if (be.recoveryTo15x !== null) {
      out.push(`- Recovery to 1.5x: **${pct(be.recoveryTo15x * 100)}** collateral price rise`);
    }
    out.push(
      baseline.isLiquidatable
        ? `- **Liquidatable now.** Max closeable in one call: ${baseline.maxLiquidatableAmount} ` +
            `(close factor ${baseline.closeFactorBps} bps)`
        : `- Not liquidatable at the current price`,
    );
  } else {
    out.push(`- Health factor **unknown**: ${baseline.unknownReason}`);
    out.push("- The contract reports 0 here, and `get_max_liquidatable_amount` returns 0, so no liquidation can proceed.");
  }

  if (report.scenarios.length > 0) {
    out.push("");
    out.push("### Historical scenarios");
    out.push("");
    out.push("| Scenario | Health factor | Risk | Max closeable |");
    out.push("| --- | ---: | --- | ---: |");
    for (const { scenario, step } of report.scenarios) {
      out.push(
        `| ${scenario.name} | ${hf(step.health.healthFactor)} | ${step.risk} | ${step.health.maxLiquidatableAmount} |`,
      );
    }
  }

  out.push("");
  out.push(`### Price grid — ${report.grid.grid.name}`);
  out.push("");
  out.push("| Collateral move | Health factor | Risk | Δ vs current | Liquidates |");
  out.push("| ---: | ---: | --- | ---: | --- |");
  for (const step of report.grid.steps) {
    const deltaPct = (Number(step.healthFactorDelta) / 10_000) * 100;
    out.push(
      `| ${step.label} | ${hf(step.health.healthFactor)} | ${step.risk} | ` +
        `${deltaPct === 0 ? "—" : `${deltaPct > 0 ? "+" : ""}${deltaPct.toFixed(2)}%`} | ` +
        `${step.becameLiquidatable ? "**yes**" : "no"} |`,
    );
  }

  out.push("");
  out.push(`### Shock heatmap — ${report.heatmap.name}`);
  out.push("");
  out.push(`_${report.heatmap.description}_`);
  out.push("");
  out.push("Rows: collateral price move · Columns: debt price move.");
  out.push("");
  {
    const { rows, cols, cells } = heatmapCells(report);
    out.push(`| Collateral \\ Debt | ${cols.join(" | ")} |`);
    out.push(`| --- | ${cols.map(() => "---:").join(" | ")} |`);
    for (let r = 0; r < rows.length; r++) {
      const parts: string[] = [rows[r]];
      for (let c = 0; c < cols.length; c++) {
        const cell = cells.get(`${r}:${c}`);
        if (!cell) {
          parts.push("—");
          continue;
        }
        const value = `${heatGlyph(cell.healthFactor)} ${hf(cell.healthFactor)}`;
        parts.push(cell.liquidatable ? `**${value}**` : value);
      }
      out.push(`| ${parts.join(" | ")} |`);
    }
    out.push("");
    out.push(
      "Legend: **X** liquidatable · ! <1.1x · ~ <1.5x · = <2.0x · # >=2.0x · ? unknown",
    );
  }

  out.push("");
  out.push(`### Liquidation threshold sweep (default ${report.thresholds.defaultBps} bps)`);
  out.push("");
  out.push("| Threshold (bps) | Health factor | Risk | Δ vs default | Liquidatable |");
  out.push("| ---: | ---: | --- | ---: | --- |");
  for (const step of report.thresholds.steps) {
    const deltaPct = (Number(step.healthFactorDelta) / 10_000) * 100;
    out.push(
      `| ${step.thresholdBps} | ${hf(step.healthFactor)} | ${step.risk} | ` +
        `${deltaPct === 0 ? "—" : `${deltaPct > 0 ? "+" : ""}${deltaPct.toFixed(2)}%`} | ` +
        `${step.isLiquidatable ? "**yes**" : "no"} |`,
    );
  }

  out.push("");
  out.push(
    `<sub>Health factor math ported from \`stellar-lend/contracts/lending/src/views.rs\`; ` +
      `constants read from ${report.constants.sources.views} and ${report.constants.sources.borrow}.</sub>`,
  );
  return out.join("\n");
}

/** JSON shape, with the bigints rendered as strings so it round-trips safely. */
export function toJson(report: SimulationReport): unknown {
  const { baseline, distance, breakEven } = report;
  return {
    positionName: report.positionName,
    constants: {
      ...report.constants,
      priceScale: report.constants.priceScale.toString(),
      healthFactorScale: report.constants.healthFactorScale.toString(),
      healthFactorNoDebt: report.constants.healthFactorNoDebt.toString(),
      defaultLiquidationThresholdBps: report.constants.defaultLiquidationThresholdBps.toString(),
      defaultCloseFactorBps: report.constants.defaultCloseFactorBps.toString(),
      defaultLiquidationIncentiveBps: report.constants.defaultLiquidationIncentiveBps.toString(),
      collateralRatioMinBps: report.constants.collateralRatioMinBps.toString(),
    },
    baseline: {
      collateralValue: baseline.collateralValue.toString(),
      debtValue: baseline.debtValue.toString(),
      totalDebtAmount: baseline.totalDebtAmount.toString(),
      healthFactor: baseline.healthFactor.toString(),
      healthFactorRatio: healthFactorToNumber(baseline.healthFactor),
      risk: report.baselineRisk,
      isLiquidatable: baseline.isLiquidatable,
      healthFactorKnown: baseline.healthFactorKnown,
      unknownReason: baseline.unknownReason,
      maxLiquidatableAmount: baseline.maxLiquidatableAmount.toString(),
      liquidationThresholdBps: baseline.liquidationThresholdBps.toString(),
      closeFactorBps: baseline.closeFactorBps.toString(),
      liquidationIncentiveBps: baseline.liquidationIncentiveBps.toString(),
    },
    breakEven,
    distanceToLiquidation: distance,
    scenarios: report.scenarios.map(({ scenario, step, unmatched }) => ({
      id: scenario.id,
      name: scenario.name,
      source: scenario.source,
      healthFactor: step.health.healthFactor.toString(),
      healthFactorRatio: healthFactorToNumber(step.health.healthFactor),
      risk: step.risk,
      isLiquidatable: step.health.isLiquidatable,
      maxLiquidatableAmount: step.health.maxLiquidatableAmount.toString(),
      unmatchedAssets: unmatched,
    })),
    priceGrid: {
      id: report.grid.grid.id,
      name: report.grid.grid.name,
      steps: report.grid.steps.map((step) => ({
        collateralChangePercent: step.collateralChangePercent,
        healthFactor: step.health.healthFactor.toString(),
        healthFactorRatio: healthFactorToNumber(step.health.healthFactor),
        risk: step.risk,
        healthFactorDelta: step.healthFactorDelta.toString(),
        becameLiquidatable: step.becameLiquidatable,
      })),
    },
    thresholdSweep: {
      defaultBps: report.thresholds.defaultBps.toString(),
      steps: report.thresholds.steps.map((step) => ({
        thresholdBps: step.thresholdBps.toString(),
        healthFactor: step.healthFactor.toString(),
        healthFactorRatio: healthFactorToNumber(step.healthFactor),
        risk: step.risk,
        isLiquidatable: step.isLiquidatable,
        healthFactorDelta: step.healthFactorDelta.toString(),
      })),
    },
    thresholdAtLiquidation: report.thresholdAtLiquidation?.toString() ?? null,
    heatmap: {
      id: report.heatmap.id,
      name: report.heatmap.name,
      rowLabels: report.heatmap.rowLabels,
      colLabels: report.heatmap.colLabels,
      cells: report.heatmap.cells.map((cell) => ({
        rowIndex: cell.rowIndex,
        colIndex: cell.colIndex,
        collateralChangePercent: cell.collateralChangePercent,
        debtChangePercent: cell.debtChangePercent,
        healthFactor: cell.healthFactor.toString(),
        healthFactorRatio: healthFactorToNumber(cell.healthFactor),
        risk: cell.risk,
        isLiquidatable: cell.isLiquidatable,
      })),
    },
  };
}
