import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

import {
  estimateStructBytes,
  indexContractTypes,
  indexFunctions,
  indexUseAliases,
  matchDelimiter,
  stripCommentsAndLiterals,
} from "./rust-source.ts";
import { allEnumVariants, indexContract, resolveAllOperations, resolveOperationFacts } from "./storage-indexer.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FIXTURES = path.join(HERE, "fixtures");

/** Scratch contracts are built outside the repo so a failing test cannot leave files behind. */
const scratch = (): string => fs.mkdtempSync(path.join(os.tmpdir(), "lending-gas-estimator-"));
const REPO_ROOT = path.resolve(HERE, "..", "..");
const REAL_CONTRACT = path.join(REPO_ROOT, "stellar-lend/contracts/lending");

const poolSource = fs.readFileSync(path.join(FIXTURES, "fixture-pool.rs"), "utf8");
const contractSource = fs.readFileSync(path.join(FIXTURES, "fixture-contract.rs"), "utf8");

test("stripCommentsAndLiterals preserves line numbers", () => {
  const source = "line one // comment\nline two /* block\nspanning */\nline three";
  const stripped = stripCommentsAndLiterals(source);
  assert.equal(stripped.split("\n").length, source.split("\n").length);
  assert.ok(!stripped.includes("comment"));
  assert.ok(!stripped.includes("spanning"));
  assert.ok(stripped.includes("line three"));
});

test("stripCommentsAndLiterals leaves lifetimes and // inside strings alone", () => {
  const stripped = stripCommentsAndLiterals("fn f<'a>(x: &'a str) { let u = \"http://x\"; }");
  assert.ok(stripped.includes("'a"), "lifetime must survive");
  assert.ok(!stripped.includes("http"), "string contents must be blanked");
});

test("stripCommentsAndLiterals handles raw strings containing quotes", () => {
  const stripped = stripCommentsAndLiterals('let s = r#"a "quoted" value"#;');
  assert.ok(!stripped.includes("quoted"));
  assert.ok(stripped.includes("let s ="));
});

test("matchDelimiter returns the matching close and survives imbalance", () => {
  assert.equal(matchDelimiter("{ a(b) }", 0), 7);
  assert.equal(matchDelimiter("(x", 0), 2, "unbalanced input returns the end of input");
});

test("indexFunctions finds free functions and impl methods with their owners", () => {
  const fns = indexFunctions(poolSource, "pool.rs");
  const byName = new Map(fns.map((f) => [f.name, f]));

  assert.ok(byName.has("is_paused"));
  assert.equal(byName.get("is_paused")?.implType, "");
  assert.equal(byName.get("load")?.implType, "PackedSlot");
  assert.equal(byName.get("commit")?.implType, "PackedSlot");
  assert.equal(byName.get("new")?.implType, "FlashGuard");
  assert.equal(byName.get("drop")?.implType, "FlashGuard");
  assert.equal(byName.get("drop")?.implTrait, "Drop", "impl Drop for T must record the trait");
  assert.ok(!byName.has("fn"), "the fn keyword itself is not a definition");
});

test("indexFunctions captures parameter text for written-value resolution", () => {
  const fns = indexFunctions(poolSource, "pool.rs");
  const commit = fns.find((f) => f.name === "commit");
  assert.ok(commit);
  assert.ok(commit.params.includes("env"), `params were: ${commit.params}`);
});

test("indexContractTypes indexes every attributed type, including contracterror", () => {
  const { structs, enums } = indexContractTypes(poolSource, "pool.rs");
  assert.deepEqual(enums.get("PoolDataKey")?.variants, ["Admin", "Total", "Cap", "UserPosition"]);
  assert.deepEqual(enums.get("PoolError")?.variants, ["Unauthorized", "Paused"]);
  assert.deepEqual(
    structs.get("PackedState")?.fields.map((f) => f.name),
    ["total", "cap"],
  );
});

test("indexContractTypes indexes unannotated structs so self.field writes resolve", () => {
  const { structs } = indexContractTypes(poolSource, "pool.rs");
  assert.equal(structs.get("PackedSlot")?.fields.find((f) => f.name === "state")?.type, "PackedState");
});

test("estimateStructBytes sums primitive widths and bails on variable-width fields", () => {
  const { structs } = indexContractTypes(poolSource, "pool.rs");
  assert.equal(estimateStructBytes(structs.get("PackedState")!, { structs, enums: new Map() }), 32);
  assert.equal(estimateStructBytes(structs.get("Limits")!, { structs, enums: new Map() }), 32);

  const wide = {
    name: "Wide",
    file: "x.rs",
    line: 1,
    fields: [{ name: "note", type: "String" }],
  };
  assert.equal(estimateStructBytes(wide, { structs, enums: new Map() }), null);
});

test("indexUseAliases maps renamed imports to module::name", () => {
  const aliases = indexUseAliases(contractSource);
  assert.equal(aliases.get("batch_apply_logic"), "pool::batch_apply");
  assert.equal(aliases.get("is_paused"), "pool::is_paused");
});

test("indexContract discovers entry points declared inside #[contractimpl]", () => {
  const dir = scratch();
  try {
    fs.writeFileSync(path.join(dir, "lib.rs"), contractSource);
    fs.writeFileSync(path.join(dir, "pool.rs"), poolSource);
    const index = indexContract(dir, { contract: "fixture" });
    const names = index.entryPoints.map((f) => f.name).sort();
    assert.deepEqual(names, ["batch", "compressed", "deposit", "flash", "limits", "note", "probe"]);
    // Impl methods are not entry points.
    assert.ok(!names.includes("commit"));
    assert.ok(!names.includes("load"));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("indexContract skips test files so they never inflate on-chain cost", () => {
  const dir = scratch();
  try {
    fs.writeFileSync(path.join(dir, "lib.rs"), contractSource);
    fs.writeFileSync(path.join(dir, "pool.rs"), poolSource);
    fs.writeFileSync(
      path.join(dir, "pool_test.rs"),
      "pub fn set_up(env: &Env) { env.storage().persistent().set(&K::A, &1); }\n",
    );
    const index = indexContract(dir, { contract: "fixture" });
    assert.ok(!index.functions.some((f) => f.name === "set_up"), "test helper must be excluded");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

function fixtureIndex() {
  const dir = scratch();
  fs.writeFileSync(path.join(dir, "lib.rs"), contractSource);
  fs.writeFileSync(path.join(dir, "pool.rs"), poolSource);
  const index = indexContract(dir, { contract: "fixture" });
  return { index, cleanup: () => fs.rmSync(dir, { recursive: true, force: true }) };
}

function factsFor(operation: string) {
  const { index, cleanup } = fixtureIndex();
  try {
    const entry = index.entryPoints.find((f) => f.name === operation);
    assert.ok(entry, `entry point ${operation} not found`);
    return resolveOperationFacts(index, entry);
  } finally {
    cleanup();
  }
}

function kinds(facts: ReturnType<typeof factsFor>, kind: string) {
  return facts.accesses.filter((a) => a.kind === kind);
}

test("direct and aliased storage handles resolve to the same tier", () => {
  const facts = factsFor("deposit");
  // lib.rs::deposit -> pool::is_paused (has + get on persistent)
  const reads = kinds(facts, "read").filter((a) => a.via === "pool::is_paused");
  assert.equal(reads.length, 1, "one get from is_paused");
  assert.ok(reads.every((a) => a.tier === "persistent"));
  // The `let storage = env.storage().persistent();` alias inside is_paused
  assert.equal(reads[0].key, "PoolDataKey::Cap");
});

test("turfish between the method name and its arguments is handled", () => {
  const facts = factsFor("limits");
  const reads = kinds(facts, "read");
  assert.ok(
    reads.some((a) => a.key === "PoolDataKey::Cap"),
    "storage.get(&PoolDataKey::Cap) behind a turbofish must be indexed",
  );
});

test("a packed write through &self.field is sized from the struct declaration", () => {
  const facts = factsFor("deposit");
  const write = kinds(facts, "write").find((a) => a.key === "PoolDataKey::Total");
  assert.ok(write, "PackedSlot::commit writes PoolDataKey::Total");
  assert.equal(write.entryBytes, 32, "PackedState is two i128 fields");
});

test("RAII drop-body writes are counted even though nothing calls drop", () => {
  const facts = factsFor("flash");
  const instanceWrites = kinds(facts, "write").filter((a) => a.tier === "instance");
  // FlashGuard::new writes two flags and Drop writes them back.
  assert.equal(instanceWrites.length, 4);
  assert.ok(instanceWrites.some((a) => (a.via ?? "").includes("(Drop)")));
});

test("outbound calls are counted as cross-contract, not as internal helpers", () => {
  const facts = factsFor("flash");
  const kindsSeen = facts.crossContractCalls.map((c) => c.kind).sort();
  assert.deepEqual(kindsSeen, ["invoke_contract", "token_client"]);
});

test("storage inside a loop body is flagged perIteration", () => {
  const facts = factsFor("batch");
  assert.equal(facts.hasLoop, true);
  const looped = kinds(facts, "write").filter((a) => a.perIteration);
  assert.equal(looped.length, 1, "the per-asset position write is inside the loop");
  // The shared total write is after the loop and must not be flagged.
  const total = kinds(facts, "write").find((a) => a.key === "PoolDataKey::Total");
  assert.equal(total?.perIteration, false);
});

test("a function without a loop reports no loop and no per-iteration access", () => {
  const facts = factsFor("deposit");
  assert.equal(facts.hasLoop, false);
  assert.ok(facts.accesses.every((a) => a.perIteration === false));
});

test("has() guarding a get() on the same variable key is recorded", () => {
  const facts = factsFor("probe");
  assert.equal(kinds(facts, "exists").length, 1);
  assert.equal(kinds(facts, "read").length, 1);
  // A variable key has no namespace, so it is never a packing candidate.
  assert.equal(kinds(facts, "exists")[0].keyEnum, "");
});

test("storage mentioned in a string or a comment is not indexed", () => {
  const facts = factsFor("note");
  assert.equal(facts.accesses.length, 0, "the only storage in this function is inside a string");
  assert.equal(facts.hasLoop, false);
});

test("an entry point with no storage access still resolves", () => {
  const facts = factsFor("compressed");
  assert.equal(facts.accesses.length, 0);
  assert.equal(facts.crossContractCalls.length, 0);
  assert.equal(facts.operation, "compressed");
});

test("a helper reached twice is priced twice", () => {
  const facts = factsFor("batch");
  // The entry point checks the pause flag, then batch_apply checks it again.
  const probes = facts.accesses.filter((a) => a.via === "pool::is_paused");
  assert.equal(probes.length, 4, "is_paused does a has() and a get(), called twice");
});

test("enum-variant constructors are not reported as unresolved calls", () => {
  // `PoolDataKey::UserPosition(..)` and `PoolError::Paused` sit in call position
  // everywhere in real contracts; treating them as missing definitions would
  // bury the calls that genuinely are missing.
  const facts = factsFor("deposit");
  assert.deepEqual(facts.unresolvedCalls, []);
});

test("calls with no definition in the scanned tree are surfaced", () => {
  const index = indexContract(REAL_CONTRACT, { contract: "lending" });
  const entry = index.entryPoints.find((f) => f.name === "deposit")!;
  const facts = resolveOperationFacts(index, entry);
  assert.ok(
    facts.unresolvedCalls.includes("ReentrancyGuard::new"),
    `the reentrancy guard lives in another crate and must be reported; unresolved: ${facts.unresolvedCalls.join(", ")}`,
  );
});

test("resolveAllOperations filters by operation name", () => {
  const { index, cleanup } = fixtureIndex();
  try {
    const all = resolveAllOperations(index);
    assert.equal(all.length, 7);
    const some = resolveAllOperations(index, { operations: ["deposit", "flash"] });
    assert.deepEqual(some.map((f) => f.operation).sort(), ["deposit", "flash"]);
  } finally {
    cleanup();
  }
});

test("maxDepth bounds the walk", () => {
  const { index, cleanup } = fixtureIndex();
  try {
    const entry = index.entryPoints.find((f) => f.name === "deposit")!;
    const shallow = resolveOperationFacts(index, entry, { maxDepth: 0 });
    assert.equal(shallow.callees.length, 0);
    assert.equal(kinds(shallow, "read").length, 0, "only the entry point body is scanned");
  } finally {
    cleanup();
  }
});

test("indexContract throws when the directory has no Rust sources", () => {
  const dir = scratch();
  try {
    assert.throws(() => indexContract(dir), /No Rust sources/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("allEnumVariants spans error and key enums", () => {
  const { index, cleanup } = fixtureIndex();
  try {
    const variants = allEnumVariants(index.types);
    assert.ok(variants.has("UserPosition"));
    assert.ok(variants.has("Unauthorized"));
  } finally {
    cleanup();
  }
});

// ── The real contract ───────────────────────────────────────────────────────

test("the real lending contract is indexed and priced on every hot-path entry point", () => {
  const index = indexContract(REAL_CONTRACT, { contract: "lending" });
  const names = new Set(index.entryPoints.map((f) => f.name));
  for (const op of [
    "deposit",
    "deposit_batch",
    "withdraw",
    "borrow",
    "repay",
    "emergency_withdraw",
    "flash_loan",
  ]) {
    assert.ok(names.has(op), `missing entry point ${op}`);
  }

  const facts = resolveAllOperations(index, {
    operations: ["deposit", "deposit_batch", "withdraw", "borrow", "repay", "flash_loan"],
  });

  const deposit = facts.find((f) => f.operation === "deposit")!;
  const depositWrites = deposit.accesses.filter((a) => a.kind === "write");
  // save_deposit_position + DepositHotSlot::commit
  assert.deepEqual(
    depositWrites.map((a) => a.key).sort(),
    ["DepositDataKey::UserCollateral(user.clone())", "HotStorageKey::DepositState"],
  );
  // The packed slot is three i128 fields.
  assert.equal(depositWrites.find((a) => a.key === "HotStorageKey::DepositState")?.entryBytes, 48);
  // The legacy fallback reads three per-field entries before the packed one.
  assert.equal(
    deposit.accesses.filter((a) => a.via === "DepositHotSlot::load" && a.kind === "read").length,
    4,
  );

  // deposit_batch does its per-item work in memory, so its storage footprint
  // matches a single deposit regardless of batch size.
  const batch = facts.find((f) => f.operation === "deposit_batch")!;
  assert.equal(batch.accesses.length, deposit.accesses.length);
  assert.equal(batch.hasLoop, true, "the validation and event passes are loops");

  const flash = facts.find((f) => f.operation === "flash_loan")!;
  assert.equal(
    flash.accesses.filter((a) => a.tier === "instance" && a.kind === "write").length,
    4,
    "FlashLoanGuard arms and disarms two instance flags",
  );
  assert.equal(flash.crossContractCalls.length, 2, "token transfer plus the receiver callback");

  const repay = facts.find((f) => f.operation === "repay")!;
  assert.ok(repay.accesses.some((a) => a.key === "HotStorageKey::DepositState") === false);
  assert.ok(repay.accesses.some((a) => a.key.startsWith("BorrowDataKey::")), "repay touches debt keys");

  // Every operation must have a distinct, non-empty entry point name.
  for (const fact of facts) {
    assert.ok(fact.entryPoint.startsWith("lending::"), fact.entryPoint);
  }
});

test("the indexer never reports a negative count or a zero-key write", () => {
  const index = indexContract(REAL_CONTRACT, { contract: "lending" });
  for (const facts of resolveAllOperations(index)) {
    for (const access of facts.accesses) {
      assert.ok(access.key.length > 0, `${facts.operation} has an empty key`);
      assert.ok(["persistent", "instance", "temporary"].includes(access.tier), access.tier);
      assert.ok(["read", "write", "exists", "remove"].includes(access.kind), access.kind);
      assert.ok(access.line > 0);
    }
  }
});
