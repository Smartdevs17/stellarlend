import { test } from "node:test";
import assert from "node:assert/strict";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import { DEFAULT_COST_MODEL, estimateOperation } from "./estimator.ts";
import { buildSuggestions, RULE_IDS } from "./suggestions.ts";
import { indexContract, resolveAllOperations } from "./storage-indexer.ts";
import { SEVERITY_ORDER, type OperationFacts, type StorageAccess } from "./types.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");

function access(over: Partial<StorageAccess> = {}): StorageAccess {
  return {
    tier: "persistent",
    kind: "read",
    key: "K::A",
    keyEnum: "K",
    perIteration: false,
    entryBytes: null,
    file: "src/pool.rs",
    line: 10,
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

function suggest(f: OperationFacts, iterations = 1) {
  return buildSuggestions([estimateOperation(f, { iterations })], {
    model: DEFAULT_COST_MODEL,
    iterations,
  });
}

function ids(list: { id: string }[]): string[] {
  return list.map((s) => s.id);
}

test("a two-entry write cluster in one key namespace is a packing candidate", () => {
  const list = suggest(
    facts({
      accesses: [
        access({ kind: "write", key: "EmergencyKey::TotalWithdrawn" }),
        access({ kind: "write", key: "EmergencyKey::TotalFees", line: 20 }),
      ],
    }),
  );
  const packing = list.filter((s) => s.id === "packable-key-namespace");
  assert.equal(packing.length, 1);
  assert.equal(packing[0].category, "packing");
  assert.equal(packing[0].severity, "medium");
  assert.equal(packing[0].estimatedSavingStroops, DEFAULT_COST_MODEL.storageWriteStroops);
  assert.equal(packing[0].evidence.length, 2);
  assert.ok(packing[0].detail.includes("1 write(s)"));
});

test("a four-entry cluster escalates the packing severity", () => {
  const list = suggest(
    facts({
      accesses: [
        access({ kind: "write", key: "DebtKey::UserVariable" }),
        access({ kind: "write", key: "DebtKey::UserStable", line: 11 }),
        access({ kind: "write", key: "DebtKey::Legacy", line: 12 }),
        access({ kind: "write", key: "DebtKey::Total", line: 13 }),
      ],
    }),
  );
  const packing = list.find((s) => s.id === "packable-key-namespace")!;
  assert.equal(packing.severity, "high");
  assert.equal(packing.estimatedSavingStroops, 3 * DEFAULT_COST_MODEL.storageWriteStroops);
});

test("entries from different namespaces are not proposed as one packed slot", () => {
  const list = suggest(
    facts({
      accesses: [
        access({ kind: "write", key: "DebtKey::User", keyEnum: "DebtKey" }),
        access({ kind: "write", key: "DepositKey::User", keyEnum: "DepositKey", line: 11 }),
      ],
    }),
  );
  assert.equal(ids(list).includes("packable-key-namespace"), false);
});

test("a single-entry namespace is never a packing candidate", () => {
  const list = suggest(facts({ accesses: [access({ kind: "write", key: "K::Only" })] }));
  assert.equal(ids(list).includes("packable-key-namespace"), false);
});

test("a variable key has no namespace and is never a packing candidate", () => {
  const list = suggest(
    facts({
      accesses: [
        access({ kind: "write", key: "key", keyEnum: "" }),
        access({ kind: "write", key: "key", keyEnum: "", line: 11 }),
      ],
    }),
  );
  assert.equal(ids(list).includes("packable-key-namespace"), false);
});

test("reading the same key twice is reported once with the redundant count", () => {
  const list = suggest(
    facts({
      accesses: [
        access({ key: "K::Oracle", keyEnum: "K" }),
        access({ key: "K::Oracle", keyEnum: "K", line: 30 }),
        access({ key: "K::Oracle", keyEnum: "K", line: 50 }),
      ],
    }),
  );
  const repeated = list.filter((s) => s.id === "repeated-key-read");
  assert.equal(repeated.length, 1);
  assert.equal(repeated[0].evidence.length, 3);
  assert.equal(repeated[0].severity, "medium");
  assert.equal(repeated[0].estimatedSavingStroops, 2 * DEFAULT_COST_MODEL.storageReadStroops);
});

test("one read of a key is not a repeated read", () => {
  const list = suggest(facts({ accesses: [access({ key: "K::Once" })] }));
  assert.equal(ids(list).includes("repeated-key-read"), false);
});

test("has() guarding a nearby get() or remove() is a redundant probe", () => {
  const list = suggest(
    facts({
      accesses: [
        access({ kind: "exists", key: "K::A", keyEnum: "" }),
        access({ kind: "read", key: "K::A", keyEnum: "", line: 14 }),
      ],
    }),
  );
  const probe = list.find((s) => s.id === "redundant-existence-probe")!;
  assert.ok(probe);
  assert.equal(probe.severity, "low");
  assert.equal(probe.estimatedSavingStroops, DEFAULT_COST_MODEL.storageExistsStroops);
  assert.ok(probe.detail.includes("idempotent"));
});

test("a probe in a different function is not a redundant guard", () => {
  const list = suggest(
    facts({
      accesses: [
        access({ kind: "exists", key: "K::A", via: "a::guard" }),
        access({ kind: "read", key: "K::A", via: "b::load", line: 14 }),
      ],
    }),
  );
  assert.equal(ids(list).includes("redundant-existence-probe"), false);
});

test("storage inside a loop is reported with the batch multiplier applied", () => {
  const list = suggest(
    facts({
      hasLoop: true,
      accesses: [
        access({ kind: "write", key: "K::Pos", perIteration: true }),
        access({ kind: "write", key: "K::Pos", keyEnum: "K", perIteration: true, line: 11 }),
        access({ kind: "write", key: "K::Pos", keyEnum: "K", perIteration: true, line: 12 }),
        access({ kind: "write", key: "K::Shared" }),
      ],
    }),
    10,
  );
  const loop = list.find((s) => s.id === "loop-amplified-storage")!;
  assert.ok(loop);
  assert.equal(loop.category, "loop-amplification");
  assert.equal(loop.severity, "high", "three looped writes escalate");
  assert.ok(loop.detail.includes("10 iteration(s)"));
  assert.ok(loop.detail.includes("30 write(s)"));
  assert.ok(loop.evidence.every((e) => e.includes("per iteration")));
});

test("a loop with no storage access is not reported", () => {
  const list = suggest(facts({ hasLoop: true, accesses: [access({ key: "K::A" })] }));
  assert.equal(ids(list).includes("loop-amplified-storage"), false);
});

test("four instance-tier guard writes are collapsed into one entry", () => {
  const list = suggest(
    facts({
      operation: "flash_loan",
      accesses: [
        access({ tier: "instance", kind: "write", key: "guard", keyEnum: "", via: "G::new" }),
        access({ tier: "instance", kind: "write", key: "active", keyEnum: "", via: "G::new", line: 11 }),
        access({ tier: "instance", kind: "write", key: "self.guard", keyEnum: "", via: "G::new (Drop)", line: 12 }),
        access({ tier: "instance", kind: "write", key: "self.active", keyEnum: "", via: "G::new (Drop)", line: 13 }),
      ],
    }),
  );
  const guard = list.find((s) => s.id === "instance-guard-writes")!;
  assert.ok(guard);
  assert.equal(guard.category, "instance-guard");
  assert.equal(guard.severity, "medium");
  assert.ok(guard.detail.includes("3 write(s)"));
});

test("two instance writes escalate differently from three", () => {
  const list = suggest(
    facts({
      accesses: [
        access({ tier: "instance", kind: "write", key: "a", keyEnum: "" }),
        access({ tier: "instance", kind: "write", key: "b", keyEnum: "", line: 11 }),
      ],
    }),
  );
  assert.equal(list.find((s) => s.id === "instance-guard-writes")?.severity, "low");
});

test("a legacy migration read-then-delete pair is reported with its namespaces", () => {
  const list = suggest(
    facts({
      operation: "deposit",
      accesses: [
        access({ kind: "read", key: "DepositKey::Total", keyEnum: "DepositKey", via: "Slot::load" }),
        access({ kind: "read", key: "DepositKey::Cap", keyEnum: "DepositKey", via: "Slot::load", line: 11 }),
        access({ kind: "remove", key: "DepositKey::Total", keyEnum: "DepositKey", via: "Slot::commit" }),
        access({ kind: "remove", key: "DepositKey::Cap", keyEnum: "DepositKey", via: "Slot::commit", line: 12 }),
      ],
    }),
  );
  const migration = list.find((s) => s.id === "legacy-migration-in-hot-path")!;
  assert.ok(migration);
  assert.equal(migration.category, "migration");
  assert.ok(migration.detail.includes("DepositKey"));
  assert.ok(migration.detail.includes("migrate_deposit_state"));
});

test("deletes without matching reads are not called a migration", () => {
  const list = suggest(
    facts({
      accesses: [
        access({ kind: "remove", key: "K::A" }),
        access({ kind: "remove", key: "K::B", line: 11 }),
      ],
    }),
  );
  assert.equal(ids(list).includes("legacy-migration-in-hot-path"), false);
});

test("a wide footprint is reported above the threshold only", () => {
  const narrow = suggest(
    facts({
      accesses: [access({ key: "K::A" }), access({ key: "K::B", line: 11 })],
    }),
  );
  assert.equal(ids(narrow).includes("wide-footprint"), false);

  const wide = suggest(
    facts({
      operation: "borrow",
      accesses: Array.from({ length: 10 }, (_, i) => access({ key: `K::${i}`, line: 10 + i })),
    }),
  );
  const footprint = wide.find((s) => s.id === "wide-footprint")!;
  assert.ok(footprint);
  assert.equal(footprint.severity, "high", "ten entries is the high band");
  assert.equal(footprint.evidence.length, 10);
  assert.equal(footprint.category, "footprint");
});

test("two outbound calls are reported and one is not", () => {
  const single = suggest(
    facts({ crossContractCalls: [{ kind: "invoke_contract", file: "f.rs", line: 1 }] }),
  );
  assert.equal(ids(single).includes("cross-contract-fanout"), false);

  const fanned = suggest(
    facts({
      crossContractCalls: [
        { kind: "token_client", file: "src/flash_loan.rs", line: 10, via: "flash_loan::flash_loan" },
        { kind: "invoke_contract", file: "src/flash_loan.rs", line: 20, via: "flash_loan::flash_loan" },
      ],
    }),
  );
  const fan = fanned.find((s) => s.id === "cross-contract-fanout")!;
  assert.ok(fan);
  assert.equal(fan.severity, "low");
  assert.equal(fan.estimatedSavingStroops, DEFAULT_COST_MODEL.crossContractStroops);
  assert.ok(fan.evidence[0].includes("via `flash_loan::flash_loan`"));
});

test("unresolved calls are surfaced as info with no saving estimate", () => {
  const list = suggest(facts({ unresolvedCalls: ["ReentrancyGuard::new", "Other::go"] }));
  const note = list.find((s) => s.id === "unresolved-calls")!;
  assert.ok(note);
  assert.equal(note.severity, "info");
  assert.equal(note.estimatedSavingStroops, null);
  assert.equal(note.evidence.length, 2);
});

test("a clean operation produces no findings", () => {
  const list = suggest(facts({ accesses: [access({ key: "K::A", keyEnum: "K" })] }));
  assert.deepEqual(list, []);
});

test("suggestions are ranked by severity, then by saving", () => {
  const list = suggest(
    facts({
      accesses: [
        ...Array.from({ length: 10 }, (_, i) => access({ key: `K::${i}`, line: 10 + i })),
        access({ tier: "instance", kind: "write", key: "a", keyEnum: "" }),
        access({ tier: "instance", kind: "write", key: "b", keyEnum: "", line: 90 }),
      ],
    }),
  );
  assert.ok(list.length >= 2);
  for (let i = 1; i < list.length; i++) {
    const previous = SEVERITY_ORDER[list[i - 1].severity];
    const current = SEVERITY_ORDER[list[i].severity];
    assert.ok(previous >= current, `not ranked: ${list[i - 1].severity} before ${list[i].severity}`);
  }
});

test("minSeverity hides lower findings and disabled drops a rule by id", () => {
  const operations = [
    estimateOperation(
      facts({
        accesses: Array.from({ length: 10 }, (_, i) => access({ key: `K::${i}`, line: 10 + i })),
      }),
    ),
  ];
  const options = { model: DEFAULT_COST_MODEL, iterations: 1 };

  assert.ok(buildSuggestions(operations, { ...options, minSeverity: "high" }).every((s) => s.severity === "high"));
  assert.equal(
    buildSuggestions(operations, { ...options, disabled: ["wide-footprint"] }).filter(
      (s) => s.id === "wide-footprint",
    ).length,
    0,
  );
});

test("the operations filter restricts the findings", () => {
  const operations = [
    estimateOperation(facts({ operation: "deposit", accesses: [access({ kind: "write", key: "K::A" }), access({ kind: "write", key: "K::B", line: 1 })] })),
    estimateOperation(facts({ operation: "withdraw", accesses: [access({ kind: "write", key: "K::A" }), access({ kind: "write", key: "K::B", line: 1 })] })),
  ];
  const list = buildSuggestions(operations, {
    model: DEFAULT_COST_MODEL,
    iterations: 1,
    operations: ["withdraw"],
  });
  assert.ok(list.length > 0);
  assert.ok(list.every((s) => s.operation === "withdraw"));
});

test("RULE_IDS matches the rules the engine can emit", () => {
  assert.equal(new Set(RULE_IDS).size, RULE_IDS.length, "rule ids must be unique");
  const operations = [
    estimateOperation(
      facts({
        hasLoop: true,
        unresolvedCalls: ["X::y"],
        crossContractCalls: [
          { kind: "token_client", file: "f.rs", line: 1 },
          { kind: "invoke_contract", file: "f.rs", line: 2 },
        ],
        accesses: [
          access({ kind: "write", key: "K::A" }),
          access({ kind: "write", key: "K::B", line: 1 }),
          access({ tier: "instance", kind: "write", key: "i1", keyEnum: "" }),
          access({ tier: "instance", kind: "write", key: "i2", keyEnum: "", line: 2 }),
          access({ kind: "exists", key: "K::A", line: 3 }),
          access({ kind: "read", key: "K::B", keyEnum: "K", line: 4 }),
          access({ kind: "read", key: "K::C", keyEnum: "K", line: 5 }),
          access({ kind: "remove", key: "K::B", keyEnum: "K", line: 6 }),
          access({ kind: "remove", key: "K::C", keyEnum: "K", line: 7 }),
          access({ kind: "read", key: "K::A", keyEnum: "K", perIteration: true, line: 7 }),
          access({ key: "K::D", keyEnum: "K", line: 8 }),
          access({ key: "K::D", keyEnum: "K", line: 9 }),
          access({ key: "K::D", keyEnum: "K", line: 10 }),
          access({ key: "K::E", keyEnum: "K", line: 11 }),
          access({ key: "K::F", keyEnum: "K", line: 12 }),
          access({ key: "K::G", keyEnum: "K", line: 13 }),
          access({ key: "K::H", keyEnum: "K", line: 14 }),
          access({ key: "K::I", keyEnum: "K", line: 15 }),
          access({ key: "K::J", keyEnum: "K", line: 16 }),
          access({ key: "K::L", keyEnum: "K", line: 17 }),
        ],
      }),
    ),
  ];
  const emitted = new Set(ids(buildSuggestions(operations, { model: DEFAULT_COST_MODEL, iterations: 1 })));
  // Every rule the engine can emit must appear in RULE_IDS, and every id in
  // RULE_IDS must be reachable from a maximal input.
  for (const id of emitted) assert.ok(RULE_IDS.includes(id), `${id} is missing from RULE_IDS`);
  for (const id of RULE_IDS) assert.ok(emitted.has(id), `${id} was never emitted`);
});

// ── The real contract ───────────────────────────────────────────────────────

test("the real lending contract produces findings grounded in real source lines", () => {
  const index = indexContract(path.join(REPO_ROOT, "stellar-lend/contracts/lending"), { contract: "lending" });
  const factsList = resolveAllOperations(index, {
    operations: ["deposit", "deposit_batch", "withdraw", "borrow", "repay", "flash_loan"],
  });
  const list = buildSuggestions(factsList.map((f) => estimateOperation(f)), {
    model: DEFAULT_COST_MODEL,
    iterations: 1,
  });

  assert.ok(list.length > 0, "the hot paths must produce findings");
  for (const suggestion of list) {
    assert.ok(RULE_IDS.includes(suggestion.id));
    assert.ok(suggestion.title.length > 0);
    assert.ok(suggestion.detail.length > 40, `${suggestion.id} has no usable explanation`);
    assert.ok(suggestion.operation.length > 0);
    assert.ok(suggestion.evidence.length > 0, `${suggestion.id} has no evidence`);
    for (const item of suggestion.evidence) {
      if (suggestion.id === "unresolved-calls") {
        // An unresolved call has no definition in the tree, so it is cited by
        // name only.
        assert.match(item, /`[A-Za-z_][\w:]*\(\)/);
        continue;
      }
      assert.match(item, /src\/[\w-]+\.rs:\d+/, `evidence must cite a real source line: ${item}`);
    }
  }

  // The known patterns in the tree must be found, by rule.
  const byId = new Set(list.map((s) => `${s.operation}:${s.id}`));
  assert.ok(
    byId.has("flash_loan:instance-guard-writes"),
    `expected the FlashLoanGuard instance writes, got ${[...byId].join(", ")}`,
  );
  assert.ok(
    byId.has("deposit:legacy-migration-in-hot-path"),
    "DepositHotSlot::commit deletes the three legacy keys from the hot path",
  );
  assert.ok(byId.has("borrow:wide-footprint"), "borrow spans the widest footprint");
  assert.ok(
    list.some((s) => s.id === "repeated-key-read"),
    "the pause flag is read twice per deposit",
  );
  assert.ok(
    list.some((s) => s.id === "packable-key-namespace"),
    "deposit mutates two entries of the BorrowDataKey namespace during repay/borrow",
  );
  assert.ok(list.some((s) => s.id === "unresolved-calls"), "ReentrancyGuard lives in another crate");
});

test("the flash loan instance-guard saving matches the two removable writes", () => {
  const index = indexContract(path.join(REPO_ROOT, "stellar-lend/contracts/lending"), { contract: "lending" });
  const factsList = resolveAllOperations(index, { operations: ["flash_loan"] });
  const list = buildSuggestions(factsList.map((f) => estimateOperation(f)), {
    model: DEFAULT_COST_MODEL,
    iterations: 1,
  });
  const guard = list.find((s) => s.id === "instance-guard-writes")!;
  // Four instance writes across two keys; packing to one removes three.
  assert.equal(guard.estimatedSavingStroops, 3 * DEFAULT_COST_MODEL.storageWriteStroops);
});
