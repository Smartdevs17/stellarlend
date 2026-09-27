import { test } from "node:test";
import assert from "node:assert/strict";

import {
  DEFAULT_COST_MODEL,
  STROOPS_PER_XLM,
  auditComplexityTable,
  baselineFor,
  buildCaveats,
  buildReport,
  estimateOperation,
  parseBaselines,
  parseComplexityTable,
  parseSharedCostConstants,
  priceOperation,
  renderMarkdown,
  renderText,
  summariseStorage,
} from "./estimator.ts";
import type { OperationFacts, StorageAccess } from "./types.ts";

function access(over: Partial<StorageAccess> = {}): StorageAccess {
  return {
    tier: "persistent",
    kind: "read",
    key: "K::A",
    keyEnum: "K",
    perIteration: false,
    entryBytes: null,
    file: "src/pool.rs",
    line: 1,
    ...over,
  };
}

function facts(over: Partial<OperationFacts> = {}): OperationFacts {
  return {
    operation: "deposit",
    entryPoint: "lending::deposit",
    file: "src/lib.rs",
    line: 1,
    accesses: [],
    crossContractCalls: [],
    callees: [],
    unresolvedCalls: [],
    hasLoop: false,
    ...over,
  };
}

test("STROOPS_PER_XLM matches the Stellar denomination", () => {
  assert.equal(STROOPS_PER_XLM, 10_000_000);
});

test("summariseStorage counts each kind and tier separately", () => {
  const summary = summariseStorage(
    facts({
      accesses: [
        access({ kind: "read", key: "K::A" }),
        access({ kind: "read", key: "K::A" }),
        access({ kind: "write", key: "K::B", entryBytes: 32 }),
        access({ kind: "exists", key: "K::C" }),
        access({ kind: "remove", key: "K::A" }),
        access({ tier: "instance", kind: "write", key: "guard" }),
      ],
    }),
  );
  assert.equal(summary.reads, 2);
  assert.equal(summary.writes, 2);
  assert.equal(summary.exists, 1);
  assert.equal(summary.removes, 1);
  assert.equal(summary.persistent, 5);
  assert.equal(summary.instance, 1);
  assert.equal(summary.temporary, 0);
  assert.equal(summary.footprintEntries, 4, "A, B, C and the instance guard");
  assert.equal(summary.writtenEntries, 2);
  assert.deepEqual(summary.ledger, { reads: 2, writes: 2, exists: 1, removes: 1 });
  assert.deepEqual(summary.scratch, { reads: 0, writes: 0, exists: 0, removes: 0 });
});

test("temporary entries are counted separately from the rent-bearing tiers", () => {
  const summary = summariseStorage(
    facts({
      accesses: [
        access({ kind: "read", key: "K::A" }),
        access({ tier: "temporary", kind: "write", key: "K::T" }),
        access({ tier: "temporary", kind: "read", key: "K::T2" }),
      ],
    }),
  );
  assert.deepEqual(summary.ledger, { reads: 1, writes: 0, exists: 0, removes: 0 });
  assert.deepEqual(summary.scratch, { reads: 1, writes: 1, exists: 0, removes: 0 });
  assert.equal(summary.temporary, 2);
  assert.equal(summary.footprintEntries, 3, "a read, a scratch write and a scratch read");
});

test("summariseStorage multiplies per-iteration accesses by the batch size", () => {
  const base = summariseStorage(
    facts({ accesses: [access({ kind: "write", key: "K::Pos", perIteration: true })] }),
    1,
  );
  const batched = summariseStorage(
    facts({ accesses: [access({ kind: "write", key: "K::Pos", perIteration: true })] }),
    20,
  );
  assert.equal(base.writes, 1);
  assert.equal(batched.writes, 20);
  assert.equal(batched.footprintEntries, 1, "one key written N times is still one footprint entry");
});

test("estimatedWriteBytes is null when any written value cannot be sized", () => {
  const known = summariseStorage(
    facts({ accesses: [access({ kind: "write", key: "K::A", entryBytes: 48 })] }),
  );
  assert.equal(known.estimatedWriteBytes, 48);

  const partial = summariseStorage(
    facts({
      accesses: [
        access({ kind: "write", key: "K::A", entryBytes: 48 }),
        access({ kind: "write", key: "K::B", entryBytes: null }),
      ],
    }),
  );
  assert.equal(partial.estimatedWriteBytes, null, "a partial total would be misleading");
});

test("priceOperation applies each component of the cost model", () => {
  const storage = summariseStorage(
    facts({
      accesses: [
        access({ kind: "write", key: "K::A" }),
        access({ kind: "read", key: "K::A" }),
        access({ kind: "exists", key: "K::A" }),
        access({ kind: "remove", key: "K::A" }),
        access({ tier: "temporary", kind: "write", key: "K::T" }),
      ],
    }),
  );
  const priced = priceOperation(storage, facts({ crossContractCalls: [{ kind: "invoke_contract", file: "f", line: 1 }] }), DEFAULT_COST_MODEL);

  assert.equal(priced.baseFeeStroops, 100);
  assert.equal(priced.storageWriteStroops, 10_000, "the temporary write is not rent-bearing");
  assert.equal(priced.storageReadStroops, 1_000);
  assert.equal(priced.storageExistsStroops, 500);
  assert.equal(priced.storageRemoveStroops, 1_000);
  assert.equal(priced.crossContractStroops, 5_000);
  assert.equal(priced.resourceStroops, 0, "no baseline supplied");
  const expected = 100 + 10_000 + 1_000 + 500 + 1_000 + 20_000 + 5_000;
  assert.equal(priced.totalStroops, expected);
  assert.equal(priced.totalXlm, expected / STROOPS_PER_XLM);
});

test("the cost model is honoured end to end", () => {
  const operation = estimateOperation(
    facts({ accesses: [access({ kind: "write", key: "K::A" })] }),
    { model: { baseFeeStroops: 1, storageWriteStroops: 2, crossContractStroops: 3 } },
  );
  assert.equal(operation.cost.baseFeeStroops, 1);
  assert.equal(operation.cost.storageWriteStroops, 2);
  assert.equal(operation.cost.totalStroops, 3);
});

test("a measured CPU and memory baseline adds a resource term", () => {
  const operation = estimateOperation(facts({ accesses: [access({ kind: "read", key: "K::A" })] }), {
    baselines: {
      deposit: { cpuInstructions: 354_765, memoryBytes: 58_682, source: "gas-baseline.json" },
    },
    model: { cpuStroopDivisor: 100, memoryStroopDivisor: 100 },
  });
  assert.equal(operation.cost.resourceStroops, 3547 + 586);
  assert.equal(operation.cost.totalStroops, 100 + 1000 + 3547 + 586);
  assert.equal(operation.baseline.source, "gas-baseline.json");
});

test("a cost model with no baseline leaves the resource term at zero", () => {
  const operation = estimateOperation(facts());
  assert.equal(operation.cost.resourceStroops, 0);
  assert.equal(operation.cost.totalStroops, DEFAULT_COST_MODEL.baseFeeStroops);
});

test("buildReport orders operations by cost and totals the storage counts", () => {
  const report = buildReport([
    facts({ operation: "cheap", accesses: [access({ kind: "read", key: "K::A" })] }),
    facts({
      operation: "dear",
      accesses: [access({ kind: "write", key: "K::A" }), access({ kind: "write", key: "K::B" })],
    }),
  ]);
  assert.deepEqual(report.operations.map((o) => o.operation), ["dear", "cheap"]);
  assert.equal(report.totals.operations, 2);
  assert.equal(report.totals.reads, 1);
  assert.equal(report.totals.writes, 2);
  assert.equal(report.totals.totalStroops, report.operations.reduce((n, o) => n + o.cost.totalStroops, 0));
  assert.equal(report.operations[0].cost.totalStroops, report.totals.totalStroops - 1_100);
});

test("buildReport rounds JSON-serialisable output for every operation", () => {
  const report = buildReport([facts({ accesses: [access({ kind: "write", key: "K::A" })] })]);
  const roundTripped = JSON.parse(JSON.stringify(report)) as typeof report;
  assert.deepEqual(roundTripped, report);
});

test("iterations multiply the per-iteration accesses in the report", () => {
  const looped = facts({
    operation: "deposit_batch",
    hasLoop: true,
    accesses: [access({ kind: "write", key: "K::Pos", perIteration: true })],
  });
  const single = buildReport([looped], { iterations: 1 });
  const twenty = buildReport([looped], { iterations: 20 });
  assert.equal(single.operations[0].storage.writes, 1);
  assert.equal(twenty.operations[0].storage.writes, 20);
  assert.ok(twenty.totals.totalStroops > single.totals.totalStroops);
});

test("parseComplexityTable reads the API's hand-maintained table", () => {
  const source = `
const BASE_FEE = '100';
const STORAGE_WRITE_COST = '10000';
const CROSS_CONTRACT_CALL_COST = '5000';

const OPERATION_COMPLEXITY = {
  deposit: { storageWrites: 2, crossContractCalls: 1 },
  borrow: { storageWrites: 3, crossContractCalls: 2 },
} as const;
`;
  assert.deepEqual(parseComplexityTable(source), {
    deposit: { storageWrites: 2, crossContractCalls: 1 },
    borrow: { storageWrites: 3, crossContractCalls: 2 },
  });
});

test("parseComplexityTable returns null when the table is absent or unparsable", () => {
  assert.equal(parseComplexityTable("const nothing = 1;"), null);
  assert.equal(parseComplexityTable("const OPERATION_COMPLEXITY = { deposit: {} } as const;"), null);
});

test("parseBaselines reads the committed gas-baseline.json shape", () => {
  const source = JSON.stringify({
    version: 1,
    contract: "hello-world",
    benchmarks: [
      { operation: "deposit_collateral", scenario: "write_cold", cpu_insns: 354765, mem_bytes: 58682 },
      { operation: "deposit_collateral", scenario: "write_warm", cpu_insns: 408310, mem_bytes: 57836 },
      { operation: "borrow_asset", scenario: "write", cpu_insns: 244830, mem_bytes: 32789 },
    ],
  });
  const parsed = parseBaselines(source, "gas-baseline.json");

  assert.deepEqual(Object.keys(parsed).sort(), ["borrow_asset", "deposit_collateral"]);
  // Two scenarios for the same call: the median stands in for either.
  assert.equal(parsed.deposit_collateral.cpuInstructions, 381537);
  assert.equal(parsed.deposit_collateral.memoryBytes, 58259);
  assert.equal(parsed.borrow_asset.cpuInstructions, 244830);
  assert.equal(parsed.borrow_asset.source, "gas-baseline.json");
});

test("parseBaselines also reads a plain operation map and tolerates junk", () => {
  const parsed = parseBaselines(
    JSON.stringify({ deposit: { cpuInstructions: 1000, memoryBytes: 200 }, bogus: "nope" }),
    "inline",
  );
  assert.equal(parsed.deposit.cpuInstructions, 1000);
  assert.equal(parsed.deposit.memoryBytes, 200);
  assert.equal(parsed.bogus, undefined, "a non-object entry is skipped");
  assert.deepEqual(parseBaselines("[]", "empty"), {});
  assert.deepEqual(parseBaselines("null", "null"), {});
});

test("baselineFor falls back to the benchmark vocabulary", () => {
  const baselines = parseBaselines(
    JSON.stringify({ benchmarks: [{ operation: "execute_flash_loan", cpu_insns: 70030, mem_bytes: 13086 }] }),
    "gas-baseline.json",
  );
  assert.equal(baselineFor(baselines, "execute_flash_loan")?.cpuInstructions, 70030);
  assert.equal(baselineFor(baselines, "flash_loan")?.cpuInstructions, 70030, "alias resolves");
  assert.equal(baselineFor(baselines, "withdraw"), undefined, "withdraw aliases withdraw_collateral");
  assert.equal(baselineFor(baselines, "set_pause"), undefined);
});

test("an aliased baseline reaches the priced operation", () => {
  const operation = estimateOperation(facts({ operation: "flash_loan" }), {
    baselines: {
      execute_flash_loan: { cpuInstructions: 70_030, memoryBytes: 13_086, source: "gas-baseline.json" },
    },
    model: { cpuStroopDivisor: 100, memoryStroopDivisor: 100 },
  });
  assert.equal(operation.cost.resourceStroops, 700 + 130);
  assert.equal(operation.baseline.cpuInstructions, 70_030);
});

test("parseSharedCostConstants adopts the API's stroop constants", () => {
  const source = `
const BASE_FEE = '100';
const STORAGE_WRITE_COST = '10000';
const CROSS_CONTRACT_CALL_COST = '5000';
`;
  assert.deepEqual(parseSharedCostConstants(source), {
    baseFeeStroops: 100,
    storageWriteStroops: 10_000,
    crossContractStroops: 5_000,
  });
});

test("parseSharedCostConstants ignores constants it does not define", () => {
  assert.deepEqual(parseSharedCostConstants("const BASE_FEE = '250';"), { baseFeeStroops: 250 });
  assert.deepEqual(parseSharedCostConstants("const unrelated = 1;"), {});
});

test("auditComplexityTable reports only the operations that disagree", () => {
  const operations = [
    estimateOperation(
      facts({
        operation: "deposit",
        accesses: [access({ kind: "write", key: "K::A" }), access({ kind: "write", key: "K::B" })],
      }),
    ),
    estimateOperation(
      facts({
        operation: "borrow",
        accesses: [access({ kind: "write", key: "K::A" }), access({ kind: "write", key: "K::B" })],
        crossContractCalls: [{ kind: "invoke_contract", file: "f", line: 1 }],
      }),
    ),
    estimateOperation(facts({ operation: "repay" })),
  ];
  const drift = auditComplexityTable(operations, {
    deposit: { storageWrites: 2, crossContractCalls: 0 },
    borrow: { storageWrites: 3, crossContractCalls: 1 },
    repay: { storageWrites: 0, crossContractCalls: 0 },
  });

  assert.equal(drift.length, 1, "deposit and repay match exactly and are omitted");
  assert.deepEqual(drift[0], {
    operation: "borrow",
    tableWrites: 3,
    indexedWrites: 2,
    delta: -1,
    tableCrossContractCalls: 1,
    indexedCrossContractCalls: 1,
  });
  assert.equal(auditComplexityTable(operations, null).length, 0);
});

test("a cross-contract-only disagreement is still reported", () => {
  const operations = [
    estimateOperation(
      facts({
        operation: "flash_loan",
        accesses: [access({ kind: "write", key: "K::A" })],
        crossContractCalls: [
          { kind: "token_client", file: "f", line: 1 },
          { kind: "invoke_contract", file: "f", line: 2 },
        ],
      }),
    ),
  ];
  const drift = auditComplexityTable(operations, {
    flash_loan: { storageWrites: 1, crossContractCalls: 1 },
  });
  assert.equal(drift.length, 1);
  assert.equal(drift[0].delta, 0, "the write counts agree");
  assert.equal(drift[0].tableCrossContractCalls, 1);
  assert.equal(drift[0].indexedCrossContractCalls, 2);
});

test("buildCaveats explains the upper bound and the out-of-tree calls", () => {
  const operations = [
    estimateOperation(
      facts({
        operation: "deposit",
        hasLoop: true,
        unresolvedCalls: ["ReentrancyGuard::new"],
        accesses: [access({ kind: "read", key: "variable" })],
      }),
    ),
  ];
  const drift = [{ operation: "borrow", tableWrites: 3, indexedWrites: 9, delta: 6, tableCrossContractCalls: 2, indexedCrossContractCalls: 1 }];
  const caveats = buildCaveats(operations, drift);

  assert.ok(caveats[0].includes("upper bound"), caveats[0]);
  assert.ok(caveats.some((c) => c.includes("perIteration")), "the loop caveat must mention perIteration");
  assert.ok(caveats.some((c) => c.includes("ReentrancyGuard::new") || c.includes("outside the scanned tree")));
  assert.ok(caveats.some((c) => c.includes("OPERATION_COMPLEXITY")));
});

test("a clean report still carries the baseline caveats", () => {
  const report = buildReport([facts()]);
  assert.ok(report.caveats.length >= 3);
  assert.ok(!report.caveats.some((c) => c.includes("loop")), "no loop, no loop caveat");
});

test("renderText prints one row per operation plus the caveats", () => {
  const report = buildReport([
    facts({ operation: "deposit", accesses: [access({ kind: "write", key: "K::A" })] }),
  ], { contract: "lending", sourceDir: "stellar-lend/contracts/lending" });
  const text = renderText(report);
  assert.ok(text.includes("Lending pool gas estimate — lending"));
  assert.ok(text.includes("stellar-lend/contracts/lending"));
  assert.ok(text.includes("deposit"));
  assert.ok(text.includes("Caveats:"));
});

test("renderMarkdown emits a table, a suggestion per heading and the caveat list", () => {
  const report = buildReport([
    facts({
      operation: "borrow",
      accesses: [
        access({ kind: "write", key: "K::A" }),
        access({ kind: "write", key: "K::B" }),
        access({ kind: "read", key: "K::A" }),
        access({ kind: "read", key: "K::B" }),
        access({ kind: "read", key: "K::C" }),
        access({ kind: "read", key: "K::D" }),
        access({ kind: "read", key: "K::E" }),
        access({ kind: "read", key: "K::F" }),
      ],
    }),
  ], { contract: "lending", sourceDir: "src" });
  const md = renderMarkdown(report);

  assert.ok(md.startsWith("## Lending pool gas estimate — lending"));
  assert.ok(md.includes("| `borrow` |"), "one table row per operation");
  assert.ok(md.includes("### Storage-pattern optimization suggestions"));
  assert.ok(md.includes("### Caveats"));
  assert.ok(md.includes("scripts/lending-gas-estimator"));
  for (const suggestion of report.suggestions) {
    assert.ok(md.includes(suggestion.id), `missing rule ${suggestion.id} in the markdown body`);
  }
});

test("renderMarkdown and renderText are stable for an empty report", () => {
  const report = buildReport([], { contract: "empty" });
  assert.ok(renderText(report).includes("0 operations"));
  assert.ok(renderMarkdown(report).includes("0 operations"));
  assert.equal(report.totals.worstSeverity, null);
});
