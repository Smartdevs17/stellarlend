/**
 * Differential testing primitives (#1067).
 *
 * Runs the same set of scenarios through two independent implementations of
 * the same behavior and checks that their outputs are equivalent — the classic
 * oracle technique for proving that a reimplementation (a rewritten contract,
 * a cross-language port, an "optimized" version) behaves identically to a
 * reference implementation.
 *
 * Outputs are compared recursively:
 *   - numbers    → equal within relative `tolerance` (default 1e-6)
 *   - everything → deep-equal after stable serialization
 * Individual mismatching paths can be allowlisted (e.g. known sqrt/lookup
 * rounding) with `--allow`.
 */

import { pathToFileURL } from "node:url";
import * as fs from "node:fs";

export interface Scenario {
  name: string;
  inputs: Record<string, unknown>;
}

export interface Implementation {
  name: string;
  run: (scenario: Scenario) => unknown | Promise<unknown>;
}

export interface DiffFinding {
  scenario: string;
  path: string;
  implA: unknown;
  implB: unknown;
  kind: "mismatch" | "type-mismatch" | "missing";
}

export interface DifferentialResult {
  ok: boolean;
  scenarios: number;
  findings: DiffFinding[];
  allowedDiffs: number;
  tolerance: number;
}

/**
 * Resolve an implementation path. Absolute / module-style paths are used as
 * given; relative paths are tried against process.cwd() first and then against
 * this module's directory (so `implementations/x.ts` works from anywhere).
 */
export function resolveImplPath(path: string): string {
  if (fs.existsSync(path)) {
    return pathToFileURL(path).href; // relative to process.cwd()
  }
  // Not present in cwd — resolve relative to this module's directory so
  // `implementations/x.ts` works regardless of where the tool is invoked from.
  if (!path.startsWith("./") && !path.startsWith("../") && !path.startsWith("/")) {
    return new URL(`./${path}`, import.meta.url).href;
  }
  return new URL(path, import.meta.url).href;
}

export async function loadImplementation(path: string): Promise<Implementation> {
  const mod = await import(resolveImplPath(path));
  const run = mod.run ?? mod.default?.run ?? (typeof mod.default === "function" ? mod.default : undefined);
  if (typeof run !== "function") {
    throw new Error(`Implementation ${path} must export async run(scenario)`);
  }
  return { name: mod.name ?? path.split("/").pop() ?? path, run };
}

function stableStringify(value: unknown): string {
  const sort = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(sort);
    if (v && typeof v === "object") {
      return Object.keys(v as object)
        .sort()
        .reduce<Record<string, unknown>>((acc, k) => {
          acc[k] = sort((v as Record<string, unknown>)[k]);
          return acc;
        }, {});
    }
    return v;
  };
  return JSON.stringify(sort(value));
}

/** Relative difference between two numbers (0 when equal). */
export function relativeDiff(a: number, b: number): number {
  const denom = Math.max(Math.abs(a), Math.abs(b));
  if (denom === 0) return 0;
  return Math.abs(a - b) / denom;
}

function differs(a: unknown, b: unknown, tolerance: number): boolean {
  if (typeof a === "number" && typeof b === "number") return relativeDiff(a, b) > tolerance;
  if (a === undefined && b !== undefined) return true;
  if (b === undefined && a !== undefined) return true;
  return stableStringify(a) !== stableStringify(b);
}

/** Recursively walk both outputs, collecting leaf-level findings. */
function walk(a: unknown, b: unknown, prefix: string, tolerance: number, acc: DiffFinding[], scenario: string): void {
  if (typeof a === "number" && typeof b === "number") {
    if (relativeDiff(a, b) > tolerance) {
      acc.push({ scenario, path: prefix, implA: a, implB: b, kind: "mismatch" });
    }
    return;
  }
  if (a && typeof a === "object" && b && typeof b === "object") {
    const aObj = a as Record<string, unknown>;
    const bObj = b as Record<string, unknown>;
    const all = new Set([...Object.keys(aObj), ...Object.keys(bObj)]);
    for (const k of all) {
      const ka = aObj[k];
      const kb = bObj[k];
      if (ka === undefined || kb === undefined) {
        if (ka !== undefined || kb !== undefined) {
          acc.push({ scenario, path: `${prefix}/${k}`, implA: ka, implB: kb, kind: "missing" });
        }
        continue;
      }
      walk(ka, kb, `${prefix}/${k}`, tolerance, acc, scenario);
    }
    return;
  }
  if (typeof a !== typeof b) {
    acc.push({ scenario, path: prefix, implA: a, implB: b, kind: "type-mismatch" });
  } else if (differs(a, b, tolerance)) {
    acc.push({ scenario, path: prefix, implA: a, implB: b, kind: "mismatch" });
  }
}

export async function runDifferential(
  scenarios: Scenario[],
  implA: Implementation,
  implB: Implementation,
  opts: { tolerance?: number; allow?: string[] } = {},
): Promise<DifferentialResult> {
  const tolerance = opts.tolerance ?? 1e-6;
  const allow = new Set(opts.allow ?? []);
  const findings: DiffFinding[] = [];

  for (const scenario of scenarios) {
    const [ra, rb] = await Promise.all([implA.run(scenario), implB.run(scenario)]);
    walk(ra, rb, "$", tolerance, findings, scenario.name);
  }

  const blocked = findings.filter((f) => !allow.has(f.path));
  const allowedDiffs = findings.length - blocked.length;

  return {
    ok: blocked.length === 0,
    scenarios: scenarios.length,
    findings,
    allowedDiffs,
    tolerance,
  };
}