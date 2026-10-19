/**
 * Market-condition scenarios for the position health simulator (#1013).
 *
 * The repository already ships a historical scenario corpus at
 * `scenarios/*.json` — the 2008 crisis, the 2020 crash, the Luna/UST collapse
 * and the 3AC/FTX contagion. Nothing on `main` loads those files: the API's
 * stress tester carries its own in-memory copy of the same four events plus four
 * more. This module reads the committed JSON instead of adding a fifth copy, so
 * the corpus is used rather than duplicated.
 *
 * The loader also accepts a simple built-in shock grid, because a threshold
 * sweep needs evenly spaced price moves rather than a historical narrative.
 */

import * as fs from "node:fs";
import * as path from "node:path";

/** One asset's price move, in percent. `-50` is a 50% fall. */
export interface PriceChange {
  asset: string;
  changePercent: number;
}

/** The schema of the committed `scenarios/*.json` files. */
export interface MarketScenario {
  id: string;
  name: string;
  description?: string;
  category?: string;
  version?: string;
  created?: string;
  priceChanges: PriceChange[];
  correlationShifts?: { asset1: string; asset2: string; newCorrelation: number }[];
  volatilityMultipliers?: { asset: string; multiplier: number }[];
  cascadingLiquidation?: boolean;
  durationSteps?: number;
  tags?: string[];
}

/** A scenario after validation, with the price changes indexed by asset. */
export interface LoadedScenario {
  id: string;
  name: string;
  description: string;
  category: string;
  priceChanges: Map<string, number>;
  volatilityMultipliers: Map<string, number>;
  cascadingLiquidation: boolean;
  tags: string[];
  source: string;
}

const SCENARIO_ID = /^[a-z0-9][a-z0-9-]*$/;

function isPriceChange(value: unknown): value is PriceChange {
  if (!value || typeof value !== "object") return false;
  const entry = value as Record<string, unknown>;
  return typeof entry.asset === "string" && typeof entry.changePercent === "number" && Number.isFinite(entry.changePercent);
}

function readNumberMap(
  list: unknown,
  key: "asset" | "asset1",
): Map<string, number> {
  const out = new Map<string, number>();
  if (!Array.isArray(list)) return out;
  for (const raw of list) {
    if (!raw || typeof raw !== "object") continue;
    const entry = raw as Record<string, unknown>;
    if (typeof entry[key] !== "string" || typeof entry.multiplier !== "number") continue;
    out.set(entry[key] as string, entry.multiplier as number);
  }
  return out;
}

/**
 * Parse one `scenarios/*.json` object.
 *
 * Throws on a structurally invalid scenario rather than skipping it: a silently
 * dropped market condition is worse than a failed run, because the report would
 * claim to have tested something it did not.
 */
export function parseScenario(source: string, origin: string): LoadedScenario {
  let raw: unknown;
  try {
    raw = JSON.parse(source);
  } catch (err) {
    throw new Error(`${origin}: not valid JSON (${(err as Error).message})`);
  }
  // The committed files are single objects; an array or a scalar is a mistake
  // worth reporting rather than indexing into.
  if (Array.isArray(raw) || !raw || typeof raw !== "object") {
    throw new Error(`${origin}: expected a JSON object, got ${Array.isArray(raw) ? "an array" : typeof raw}`);
  }
  const scenario = raw as Record<string, unknown>;

  if (typeof scenario.id !== "string" || !SCENARIO_ID.test(scenario.id)) {
    throw new Error(`${origin}: "id" must be a lowercase slug, got ${JSON.stringify(scenario.id)}`);
  }
  if (typeof scenario.name !== "string" || scenario.name.length === 0) {
    throw new Error(`${origin}: "name" is required`);
  }
  if (!Array.isArray(scenario.priceChanges) || scenario.priceChanges.length === 0) {
    throw new Error(`${origin}: "priceChanges" must be a non-empty array`);
  }
  for (const change of scenario.priceChanges) {
    if (!isPriceChange(change)) {
      throw new Error(
        `${origin}: every priceChanges entry needs an "asset" and a numeric "changePercent"`,
      );
    }
  }

  const priceChanges = new Map<string, number>();
  for (const change of scenario.priceChanges) {
    if (priceChanges.has(change.asset)) {
      throw new Error(`${origin}: asset "${change.asset}" appears twice in priceChanges`);
    }
    priceChanges.set(change.asset, change.changePercent);
  }

  return {
    id: scenario.id,
    name: scenario.name,
    description: typeof scenario.description === "string" ? scenario.description : "",
    category: typeof scenario.category === "string" ? scenario.category : "uncategorised",
    priceChanges,
    volatilityMultipliers: readNumberMap(scenario.volatilityMultipliers, "asset"),
    cascadingLiquidation: scenario.cascadingLiquidation === true,
    tags: Array.isArray(scenario.tags)
      ? scenario.tags.filter((tag): tag is string => typeof tag === "string")
      : [],
    source: origin,
  };
}

/** Load every `*.json` scenario in a directory, sorted by file name. */
export function loadScenarioDir(dir: string): LoadedScenario[] {
  if (!fs.existsSync(dir)) {
    throw new Error(`scenario directory not found: ${dir}`);
  }
  const files = fs.readdirSync(dir).filter((file) => file.endsWith(".json")).sort();
  if (files.length === 0) throw new Error(`no scenario files in ${dir}`);
  return files.map((file) => parseScenario(fs.readFileSync(path.join(dir, file), "utf8"), file));
}

/** Look a scenario up by id. */
export function findScenario(scenarios: LoadedScenario[], id: string): LoadedScenario {
  const found = scenarios.find((scenario) => scenario.id === id);
  if (!found) {
    throw new Error(
      `unknown scenario "${id}"; available: ${scenarios.map((s) => s.id).sort().join(", ")}`,
    );
  }
  return found;
}

/** A named set of price moves, used when no historical scenario applies. */
export interface ShockGrid {
  id: string;
  name: string;
  /** Percent moves applied to every collateral asset, in listed order. */
  changes: number[];
  description: string;
}

/** Evenly spaced shock grids for threshold and price sweeps. */
export const SHOCK_GRIDS: Record<string, ShockGrid> = {
  gentle_downside: {
    id: "gentle_downside",
    name: "Gentle downside",
    changes: [-5, -10, -15, -20],
    description: "A routine drawdown: collateral falls a fifth, debt is unchanged.",
  },
  flash_crash: {
    id: "flash_crash",
    name: "Flash crash",
    changes: [-20, -30, -40, -50],
    description: "A single-session collapse, the shape the contract's liquidation threshold exists for.",
  },
  black_swan: {
    id: "black_swan",
    name: "Black swan",
    changes: [-40, -50, -60, -70, -80],
    description: "Collateral loses most of its value; a position this deep is past any threshold.",
  },
  both_sides: {
    id: "both_sides",
    name: "Both sides",
    changes: [-40, -20, -10, 0, 10, 20, 40],
    description: "Downside and upside, to confirm the health factor moves monotonically in both.",
  },
};

/** Shock grid names, sorted, for `--list-scenarios`. */
export function shockGridNames(): string[] {
  return Object.keys(SHOCK_GRIDS).sort();
}

/** Look a shock grid up by name. */
export function getShockGrid(name: string): ShockGrid {
  const grid = SHOCK_GRIDS[name];
  if (!grid) {
    throw new Error(`unknown shock grid "${name}"; available: ${shockGridNames().join(", ")}`);
  }
  return grid;
}
