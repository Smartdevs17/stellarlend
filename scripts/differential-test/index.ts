#!/usr/bin/env node
/**
 * Differential testing CLI (#1067).
 *
 * Runs the same scenarios through two independent implementations of the same
 * behavior and verifies their outputs agree (numbers within tolerance).
 *
 * Run (Node >= 22, no install needed):
 *   node --experimental-strip-types scripts/differential-test/index.ts \
 *     --scenarios scenarios.json \
 *     --impl-a scripts/differential-test/implementations/interest-v1.ts \
 *     --impl-b scripts/differential-test/implementations/interest-v2.ts \
 *     --tolerance 0.0001 \
 *     --out report.json
 *
 * Scenario file: JSON array of { "name", "inputs" }. Each implementation
 * module must export `async run(scenario): unknown`.
 *
 * Exit codes: 0 equivalent, 1 diverged, 2 usage error.
 */

import * as fs from "node:fs";
import { type Scenario, loadImplementation, runDifferential } from "./differential.ts";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

function main(): void {
  const scenariosPath = arg("scenarios");
  const implAPath = arg("impl-a");
  const implBPath = arg("impl-b");
  const tolRaw = arg("tolerance");
  const allowRaw = arg("allow");
  const out = arg("out");

  if (!scenariosPath || !implAPath || !implBPath) {
    console.error("Usage: differential-test --scenarios <scenarios.json> --impl-a <impl.ts> --impl-b <impl.ts> [--tolerance 1e-6] [--allow 'a,b'] [--out report.json]");
    process.exit(2);
  }

  const scenarios = JSON.parse(fs.readFileSync(scenariosPath, "utf8")) as Scenario[];
  const tolerance = tolRaw !== undefined ? Number.parseFloat(tolRaw) : 1e-6;
  const allow = allowRaw ? allowRaw.split(",").map((s) => s.trim()).filter(Boolean) : [];

  Promise.all([loadImplementation(implAPath), loadImplementation(implBPath)])
    .then(async ([implA, implB]) => {
      const result = await runDifferential(scenarios, implA, implB, { tolerance, allow });

      if (out) fs.writeFileSync(out, JSON.stringify(result, null, 2));

      console.log(`Differential: ${implA.name} vs ${implB.name} (tolerance ${tolerance})`);
      console.log(`Scenarios run: ${result.scenarios}`);
      console.log(`Divergences:   ${result.findings.length}${result.allowedDiffs ? ` (${result.allowedDiffs} allowlisted)` : ""}`);

      if (result.findings.length) {
        console.log("\nFindings:");
        for (const f of result.findings) {
          const tag = allow.includes(f.path) ? " [allowed]" : "";
          console.log(`  [${f.kind}]${tag} ${f.scenario} @ ${f.path}: ${JSON.stringify(f.implA)} vs ${JSON.stringify(f.implB)}`);
        }
      }

      if (result.ok) {
        console.log("\nOK — implementations agree for every scenario.");
        process.exit(0);
      }
      console.error("\nDIVERGED — implementations produce differing outputs.");
      process.exit(1);
    })
    .catch((err) => {
      console.error(err);
      process.exit(2);
    });
}

main();