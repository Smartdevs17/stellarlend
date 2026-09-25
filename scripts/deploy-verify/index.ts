#!/usr/bin/env node
/**
 * Contract deployment verification script (#1066).
 *
 * Validates a deployment manifest (environments/<network>/deployment.json):
 * contract addresses, per-contract uniqueness, and — when local WASM builds are
 * available — that the recorded bytecode hash matches the artifact actually built.
 *
 * Run (Node >= 22, no install needed):
 *   # Offline structural verification only
 *   node --experimental-strip-types scripts/deploy-verify/index.ts \
 *     --manifest environments/testnet/deployment.json
 *
 *   # Verify against local WASM builds (authenticity of deployed bytecode)
 *   node --experimental-strip-types scripts/deploy-verify/index.ts \
 *     --manifest environments/testnet/deployment.json \
 *     --wasm-dir stellar-lend/target/wasm32-unknown-unknown/release
 *
 *   # Recompute + write back hashes from local artifacts (before committing)
 *   node --experimental-strip-types scripts/deploy-verify/index.ts \
 *     --manifest environments/testnet/deployment.json \
 *     --wasm-dir ... --update-hashes
 *
 * Exit codes: 0 verified, 1 verification failed, 2 usage error.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { type DeploymentManifest, artifactSha256, verifyManifest } from "./verify.ts";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}
function flag(name: string): boolean {
  return process.argv.includes(`--${name}`);
}

function renderReport(report: ReturnType<typeof verifyManifest>): string {
  const lines = [`Deployment verify: ${report.manifest}`, ""];
  for (const check of report.checks) {
    lines.push(`  ${check.ok ? "PASS" : "FAIL"}  ${check.name}`);
    for (const f of check.failures) lines.push(`       - ${f}`);
  }
  lines.push("");
  lines.push(`Result: ${report.ok ? "OK" : `FAILED (${report.failureCount} contract(s))`}`);
  return lines.join("\n");
}

function main(): void {
  const manifestPath = arg("manifest");
  const wasmDir = arg("wasm-dir");
  const updateHashes = flag("update-hashes");
  const out = arg("out");

  if (!manifestPath) {
    console.error('Usage: deploy-verify --manifest <deployment.json> [--wasm-dir <dir>] [--update-hashes] [--out report.json]');
    process.exit(2);
  }
  if (!fs.existsSync(manifestPath)) {
    console.error(`Manifest not found: ${manifestPath}`);
    process.exit(2);
  }

  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8")) as DeploymentManifest;

  if (updateHashes) {
    if (!wasmDir) {
      console.error('--update-hashes requires --wasm-dir');
      process.exit(2);
    }
    for (const c of manifest.contracts) {
      const artifact = path.join(wasmDir, c.wasmArtifact);
      if (!fs.existsSync(artifact)) {
        console.error(`Cannot hash missing artifact: ${artifact}`);
        process.exit(2);
      }
      c.wasmHash = artifactSha256(artifact);
      console.log(`  ${c.name}: wasmHash -> ${c.wasmHash.slice(0, 16)}…`);
    }
    fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");
    console.log(`Hashes updated in ${manifestPath}`);
    return;
  }

  const report = verifyManifest(manifest, wasmDir);
  const text = renderReport(report);
  if (out) fs.writeFileSync(out, JSON.stringify(report, null, 2));

  if (report.ok) {
    console.log(text);
    process.exit(0);
  }
  console.error(text);
  process.exit(1);
}

main();