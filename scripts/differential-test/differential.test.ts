import { test } from "node:test";
import assert from "node:assert/strict";
import { loadImplementation, relativeDiff, runDifferential } from "./differential.ts";

const scnA = { name: "simple", inputs: { principal: 1000, rate: 0.1, years: 1 } };
const scnB = { name: "zero", inputs: { principal: 500, rate: 0, years: 5 } };

const implAdd = { name: "add", run: async (s: { inputs: { a: number; b: number } }) => ({ sum: s.inputs.a + s.inputs.b }) };
const implAddTwice = { name: "add2", run: async (s: { inputs: { a: number; b: number } }) => ({ sum: (s.inputs.a + s.inputs.b) * 2 }) };
const implOffset = { name: "offset", run: async (s: { inputs: { a: number; b: number } }) => ({ sum: s.inputs.a + s.inputs.b + 0.5 }) };

test("identical implementations pass every scenario", async () => {
  const r = await runDifferential(
    [{ name: "x", inputs: { a: 2, b: 3 } }],
    implAdd,
    { name: "add-copy", run: implAdd.run },
  );
  assert.equal(r.ok, true);
  assert.deepEqual(r.findings, []);
});

test("divergent numeric output is a finding that fails the run", async () => {
  const r = await runDifferential([{ name: "x", inputs: { a: 2, b: 3 } }], implAdd, implAddTwice);
  assert.equal(r.ok, false);
  assert.equal(r.findings.length, 1);
  assert.equal(r.findings[0].path, "$/sum");
});

test("relative tolerance gates numeric equivalence", async () => {
  const tight = await runDifferential([{ name: "x", inputs: { a: 2, b: 3 } }], implAdd, implOffset);
  assert.equal(tight.ok, false);

  const loose = await runDifferential([{ name: "x", inputs: { a: 2, b: 3 } }], implAdd, implOffset, { tolerance: 0.2 });
  assert.equal(loose.ok, true);
});

test("allowlist suppresses known divergences", async () => {
  const r = await runDifferential([{ name: "x", inputs: { a: 2, b: 3 } }], implAdd, implOffset, {
    tolerance: 1e-9,
    allow: ["$/sum"],
  });
  assert.equal(r.ok, true);
  assert.equal(r.allowedDiffs, 1);
});

test("relativeDiff computes normalized difference", () => {
  assert.equal(relativeDiff(0, 0), 0);
  assert.equal(relativeDiff(100, 100), 0);
  assert.ok(relativeDiff(100, 110) > 0.09 && relativeDiff(100, 110) < 0.11);
});

test("loadImplementation imports an async run() from a ts module", async () => {
  const impl = await loadImplementation("./implementations/interest-v1.ts");
  assert.equal(typeof impl.run, "function");
  const out = (await impl.run({ name: "x", inputs: { principal: 1000, rate: 0.1, years: 1 } })) as {
    accrued: number;
    total: number;
  };
  assert.ok(out.total > 1100 && out.total < 1106);
  assert.equal(out.count, 1);
});

test("continuous vs discrete interest implementations agree within tolerance", async () => {
  const [v1, v2] = await Promise.all([
    loadImplementation("./implementations/interest-v1.ts"),
    loadImplementation("./implementations/interest-v2.ts"),
  ]);
  const scenarios = [
    { name: "a", inputs: { principal: 1000, rate: 0.06, years: 1 } },
    { name: "b", inputs: { principal: 500000, rate: 0.11, years: 3 } },
    { name: "c", inputs: { principal: 25000, rate: 0.085, years: 10 } },
  ];
  const r = await runDifferential(scenarios, v1, v2, { tolerance: 0.0005 });
  assert.equal(r.ok, true, `divergences: ${JSON.stringify(r.findings)}`);
});