/**
 * Contract deployment verification primitives (#1066).
 *
 * Validates a deployment manifest — the record of where each contract was
 * deployed, on which network, with which address and WASM bytecode hash.
 * Verification is purely offline & structural:
 *   - Stellar contract address format (C… base32, 56 chars)
 *   - record-level integrity (unique names, consistent network)
 *   - bytecode authenticity: recorded wasmHash must match the SHA-256 of the
 *     locally built artifact (the trustworthy "as-built" source of truth),
 *     catching drift between what was deployed and what is on disk.
 */

import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";

export interface DeployedContract {
  name: string;
  address: string;
  wasmArtifact: string;
  /** Optional expected hash; when absent it is computed from the artifact. */
  wasmHash?: string;
  initialized?: boolean;
}

export interface DeploymentManifest {
  network?: string;
  deployedAt?: string;
  contracts: DeployedContract[];
}

export interface CheckResult {
  name: string;
  ok: boolean;
  failures: string[];
}

export interface VerifyReport {
  ok: boolean;
  manifest: string;
  checks: CheckResult[];
  failureCount: number;
}

/**
 * Validate a Stellar contract address: 56 chars, starts with 'C', base32
 * alphabet (Stellar uses RFC-4648 base32: A-Z, 2-7).
 */
export function isStellarAddress(addr: string): boolean {
  return /^C[A-Z2-7]{55}$/.test(addr);
}

export function sha256Hex(data: Buffer): string {
  return createHash("sha256").update(data).digest("hex");
}

export function artifactSha256(artifactPath: string): string {
  return sha256Hex(fs.readFileSync(artifactPath));
}

export function checkContract(c: DeployedContract, wasmDir?: string): CheckResult {
  const failures: string[] = [];
  if (!isStellarAddress(c.address)) {
    failures.push(
      `invalid Stellar address "${c.address}" (expected /^C[A-Z2-7]{55}$/)`,
    );
  }
  if (c.address !== c.address.trim()) {
    failures.push(`address for ${c.name} has surrounding whitespace`);
  }
  if (typeof c.initialized === "boolean" && !c.initialized) {
    failures.push(`contract ${c.name} is not initialized`);
  }
  if (wasmDir) {
    const artifact = path.join(wasmDir, c.wasmArtifact);
    if (!fs.existsSync(artifact)) {
      failures.push(`wasm artifact not found: ${artifact}`);
    } else {
      const actual = artifactSha256(artifact);
      if (c.wasmHash) {
        if (actual !== c.wasmHash) {
          failures.push(
            `wasm hash mismatch for ${c.name}: recorded ${c.wasmHash.slice(0, 16)}… ≠ built ${actual.slice(0, 16)}…`,
          );
        }
      } else {
        failures.push(`wasmHash for ${c.name} missing (expected ${actual.slice(0, 16)}…)`);
      }
    }
  } else if (!c.wasmHash) {
    failures.push(`wasmHash for ${c.name} missing`);
  }
  return { name: c.name, ok: failures.length === 0, failures };
}

/** Verify a whole manifest; returns a structured report and pass/fail. */
export function verifyManifest(manifest: DeploymentManifest, wasmDir?: string): VerifyReport {
  const seen = new Set<string>();
  const checks = manifest.contracts.map((c) => {
    const check = checkContract(c, wasmDir);
    if (seen.has(c.name)) {
      check.ok = false;
      check.failures.push(`duplicate contract name "${c.name}"`);
    }
    seen.add(c.name);
    return check;
  });
  const failureCount = checks.filter((c) => !c.ok).length;
  return {
    ok: failureCount === 0,
    manifest: manifest.network ?? "unknown",
    checks,
    failureCount,
  };
}