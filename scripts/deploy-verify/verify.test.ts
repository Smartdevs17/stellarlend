import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import {
  type DeploymentManifest,
  isStellarAddress,
  sha256Hex,
  verifyManifest,
} from "./verify.ts";

const VALID = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAL";
const OTHER = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAM";

function hex(bytes: number): string {
  return createHash("sha256").update(Buffer.alloc(bytes, 1)).digest("hex");
}

test("isStellarAddress accepts 56-char C-prefixed base32 and rejects junk", () => {
  assert.equal(isStellarAddress(VALID), true);
  assert.equal(isStellarAddress("G" + VALID.slice(1)), false); // G = account, not contract
  assert.equal(isStellarAddress(VALID.slice(0, 55)), false);
  assert.equal(isStellarAddress("C" + "0".repeat(55)), false); // base32 has no 0
  assert.equal(isStellarAddress("c" + VALID.slice(1)), false);
});

test("verifyManifest passes a clean manifest", () => {
  const manifest: DeploymentManifest = {
    network: "testnet",
    contracts: [
      { name: "lending", address: VALID, wasmHash: hex(32), initialized: true },
      { name: "amm", address: OTHER, wasmHash: hex(32), initialized: true },
    ],
  };
  const report = verifyManifest(manifest);
  assert.equal(report.ok, true);
  assert.equal(report.failureCount, 0);
});

test("verifyManifest flags bad address, dup name, uninitialized contract", () => {
  const manifest: DeploymentManifest = {
    network: "testnet",
    contracts: [
      { name: "lending", address: "not-an-address", wasmHash: hex(32), initialized: false },
      { name: "amm", address: OTHER, wasmHash: hex(32), initialized: true },
      { name: "amm", address: OTHER, wasmHash: hex(32), initialized: true },
    ],
  };
  const report = verifyManifest(manifest);
  assert.equal(report.ok, false);
  assert.equal(report.failureCount, 2);
  const failures = report.checks.flatMap((c) => c.failures);
  assert.ok(failures.some((f) => f.includes("invalid Stellar address")));
  assert.ok(failures.some((f) => f.includes("not initialized")));
  assert.ok(failures.some((f) => f.includes("duplicate contract name")));
});

test("verifyManifest detects wasm hash mismatch when artifacts are checked", () => {
  const match = sha256Hex(Buffer.from("wasm-bytes"));
  const mismatch = hex(32);
  const tmp = fs.mkdtempSync("/tmp/deploy-verify-");
  fs.writeFileSync(`${tmp}/lending.wasm`, Buffer.from("wasm-bytes"));
  fs.writeFileSync(`${tmp}/amm.wasm`, Buffer.from("wasm-bytes"));
  const manifest: DeploymentManifest = {
    network: "testnet",
    contracts: [
      { name: "lending", address: VALID, wasmArtifact: "lending.wasm", wasmHash: match },
      { name: "amm", address: OTHER, wasmArtifact: "amm.wasm", wasmHash: mismatch },
    ],
  };
  const report = verifyManifest(manifest, tmp);
  assert.equal(report.ok, false);
  const amm = report.checks.find((c) => c.name === "amm")!;
  assert.equal(amm.ok, false);
  assert.ok(amm.failures.some((f) => f.includes("wasm hash mismatch")));
  const lending = report.checks.find((c) => c.name === "lending")!;
  assert.equal(lending.ok, true);
});