/**
 * Static storage indexer for the lending pool contract (#1011).
 *
 * Reads the Soroban contract source and recovers, for every public entry point,
 * the storage accesses it performs — directly and through internal function
 * calls. This is the only source of storage counts in the repo: the Rust
 * benchmark harness hard-codes `write_entries`/`disk_read_entries` to `0`, and
 * the API's `OPERATION_COMPLEXITY` table is hand-maintained.
 *
 * The walk is intentionally conservative:
 *   - unresolved calls are reported instead of assumed free;
 *   - a function is expanded at most once per entry point, but *is* expanded
 *     once per call site so repeated calls are priced repeatedly;
 *   - accesses inside a loop are flagged `perIteration` instead of being
 *     silently multiplied by an unknown factor.
 */

import * as fs from "node:fs";
import * as path from "node:path";

import {
  estimateStructBytes,
  indexContractTypes,
  indexFunctions,
  indexUseAliases,
  matchDelimiter,
  stripCommentsAndLiterals,
  type ContractStruct,
  type ContractEnum,
  type ContractTypeIndex,
  type RustFunction,
} from "./rust-source.ts";
import type { CrossContractCall, OperationFacts, StorageAccess, StorageOpKind, StorageTier } from "./types.ts";

const IDENT = "[A-Za-z_][A-Za-z0-9_]*";
const TIER_RE = "(persistent|instance|temporary)";

const TIER_OF: Record<string, StorageTier> = {
  persistent: "persistent",
  instance: "instance",
  temporary: "temporary",
};

const KIND_OF: Record<string, StorageOpKind> = {
  get: "read",
  set: "write",
  has: "exists",
  remove: "remove",
};

/** Everything the walker needs to resolve names against a scanned source tree. */
export interface ContractIndex {
  contract: string;
  sourceDir: string;
  functions: RustFunction[];
  types: ContractTypeIndex;
  /** alias -> `module::name`, collected from every scanned file. */
  aliases: Map<string, string>;
  /** Entry points declared inside a `#[contractimpl] impl` block. */
  entryPoints: RustFunction[];
}

export interface IndexOptions {
  /** Contract name reported in the report; defaults to the directory name. */
  contract?: string;
  /** Max internal hops followed from an entry point. */
  maxDepth?: number;
}

function listRustFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === "target" || entry.name.startsWith(".")) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listRustFiles(full));
      continue;
    }
    if (!entry.name.endsWith(".rs")) continue;
    // Test and spec files never ship in the wasm, so they are never on-chain cost.
    if (/_tests?\.rs$|^test_|_(test|tests|prop_test|invariant_test_suite)\.rs$/.test(entry.name)) continue;
    out.push(full);
  }
  return out.sort();
}

/** Collect the `pub fn`s declared inside a `#[contractimpl] impl … { … }` block. */
function findEntryPointFunctions(source: string, file: string): RustFunction[] {
  const src = stripCommentsAndLiterals(source);
  const all = indexFunctions(source, file);
  const out: RustFunction[] = [];
  const re = /#\[contractimpl[\]]/g;
  for (let m = re.exec(src); m; m = re.exec(src)) {
    const implRe = new RegExp(`\\bimpl\\s+(${IDENT})\\s*(?:<[^>]*>)?\\s*\\{`, "g");
    implRe.lastIndex = m.index;
    const implMatch = implRe.exec(src);
    if (!implMatch) continue;
    const typeName = implMatch[1];
    const brace = src.indexOf("{", implMatch.index);
    const end = matchDelimiter(src, brace);
    for (const fn of all) {
      // Methods of the contract impl block, declared public: those are the
      // externally callable entry points Soroban actually generates.
      if (fn.isPublic && fn.implType === typeName) out.push(fn);
    }
  }
  return out;
}

function lineOf(src: string, offset: number): number {
  let line = 1;
  for (let i = 0; i < offset; i++) if (src[i] === "\n") line++;
  return line;
}

/** Scan a contract source tree into a resolvable index. */
export function indexContract(sourceDir: string, options: IndexOptions = {}): ContractIndex {
  const files = listRustFiles(sourceDir);
  if (files.length === 0) {
    throw new Error(`No Rust sources found under ${sourceDir}`);
  }

  const functions: RustFunction[] = [];
  const structs = new Map<string, ContractStruct>();
  const enums = new Map<string, ContractEnum>();
  const aliases = new Map<string, string>();
  let entryPoints: RustFunction[] = [];

  for (const file of files) {
    const source = fs.readFileSync(file, "utf8");
    const types = indexContractTypes(source, file);
    for (const [name, def] of types.structs) if (!structs.has(name)) structs.set(name, def);
    for (const [name, def] of types.enums) if (!enums.has(name)) enums.set(name, def);
    for (const [alias, target] of indexUseAliases(source)) if (!aliases.has(alias)) aliases.set(alias, target);
    functions.push(...indexFunctions(source, file));
    entryPoints = entryPoints.concat(findEntryPointFunctions(source, file));
  }

  // De-duplicate entry points by name, keeping the first (deterministic order).
  const seen = new Set<string>();
  const uniqueEntryPoints = entryPoints.filter((fn) => {
    if (seen.has(fn.name)) return false;
    seen.add(fn.name);
    return true;
  });

  return {
    contract: options.contract ?? path.basename(sourceDir),
    sourceDir,
    functions,
    types: { structs, enums },
    aliases,
    entryPoints: uniqueEntryPoints,
  };
}

/** Short unique id for a function definition, e.g. `deposit::save_deposit_position`. */
function functionId(fn: RustFunction): string {
  return fn.implType ? `${fn.implType}::${fn.name}` : `${path.basename(fn.file, ".rs")}::${fn.name}`;
}

/**
 * SDK and macro names that look like calls but are not resolvable internals.
 * Filtering them keeps `unresolvedCalls` meaningful instead of noisy.
 */
const SDK_NAMES = new Set([
  "Client", "Address", "Env", "String", "Vec", "Map", "Symbol", "Bytes", "I256", "Error",
  "Some", "None", "Ok", "Err", "Self", "StringN", "Duration", "IntoVal", "TryFromVal",
]);

/** Numeric primitives — `u128::from(..)` is a conversion, not a call. */
const PRIMITIVE_TYPES = new Set([
  "u8", "i8", "u16", "i16", "u32", "i32", "u64", "i64", "u128", "i128", "usize", "isize",
]);

/** Resolve a called name to candidate definitions, in confidence order. */
function resolveCallee(
  index: ContractIndex,
  name: string,
  fromFile: string,
): { matches: RustFunction[]; ambiguous: boolean } {
  if (SDK_NAMES.has(name)) return { matches: [], ambiguous: false };

  const byQualifiedName = (qualified: string): RustFunction[] => {
    const parts = qualified.split("::").filter(Boolean);
    const bare = parts[parts.length - 1];
    const owner = parts.length >= 2 ? parts[parts.length - 2] : "";
    if (!owner) return [];
    return index.functions.filter(
      (fn) =>
        fn.name === bare &&
        (fn.implType === owner || path.basename(fn.file, ".rs") === owner),
    );
  };

  const alias = index.aliases.get(name);
  if (alias) {
    const matches = byQualifiedName(alias);
    if (matches.length > 0) return { matches, ambiguous: false };
  }

  // `module::fn(...)` written inline at the call site.
  if (name.includes("::")) {
    const matches = byQualifiedName(name);
    if (matches.length > 0) return { matches, ambiguous: false };
  }

  const byName = index.functions.filter((fn) => fn.name === name);
  if (byName.length === 0) return { matches: [], ambiguous: false };
  const sameFile = byName.filter((fn) => fn.file === fromFile);
  if (sameFile.length === 1) return { matches: sameFile, ambiguous: false };
  if (byName.length === 1) return { matches: byName, ambiguous: false };
  return { matches: [sameFile[0] ?? byName[0]], ambiguous: true };
}

/** Receiver names that alias a storage tier, e.g. `let storage = env.storage().persistent();`. */
function storageAliases(body: string): Map<string, StorageTier> {
  const aliases = new Map<string, StorageTier>();
  const direct = new RegExp(`\\blet\\s+(?:mut\\s+)?(${IDENT})\\s*(?::[^=]*?)?=\\s*env\\s*\\.\\s*storage\\s*\\(\\s*\\)\\s*\\.\\s*${TIER_RE}\\s*\\(`, "g");
  for (let m = direct.exec(body); m; m = direct.exec(body)) aliases.set(m[1], TIER_OF[m[2]]);

  // `let s = env.storage();` followed by `s.persistent()`
  const storageVar = new RegExp(`\\blet\\s+(?:mut\\s+)?(${IDENT})\\s*(?::[^=]*?)?=\\s*env\\s*\\.\\s*storage\\s*\\(\\s*\\)`, "g");
  const storageVars = new Set<string>();
  for (let m = storageVar.exec(body); m; m = storageVar.exec(body)) storageVars.add(m[1]);
  if (storageVars.size > 0) {
    const qualified = new RegExp(`\\b(${[...storageVars].join("|")})\\s*\\.\\s*${TIER_RE}\\s*\\(`, "g");
    for (let m = qualified.exec(body); m; m = qualified.exec(body)) {
      aliases.set(m[1], TIER_OF[m[2]]);
    }
  }
  return aliases;
}

/** Local bindings with an explicit type, e.g. `let hot: DepositHotSlot = …`. */
function typedLocals(body: string): Map<string, string> {
  const locals = new Map<string, string>();
  const re = new RegExp(`\\blet\\s+(?:mut\\s+)?(${IDENT})\\s*:\\s*(${IDENT})\\b`, "g");
  for (let m = re.exec(body); m; m = re.exec(body)) locals.set(m[1], m[2]);
  return locals;
}

/** Parameter names with an explicit struct type, e.g. `position: &DepositCollateral`. */
function typedParams(params: string): Map<string, string> {
  const out = new Map<string, string>();
  const re = new RegExp(`(${IDENT})\\s*:\\s*&?(?:mut\\s+)?(${IDENT})\\b`, "g");
  for (let m = re.exec(params); m; m = re.exec(params)) out.set(m[1], m[2]);
  return out;
}

/**
 * Character ranges covered by a `for` / `while` / `loop` body.
 *
 * A single left-to-right pass tracking a `pendingLoop` flag: the keyword arms it,
 * the next `{` consumes it, and `;` or an intervening brace clears it. This is
 * what lets the indexer flag a storage access as per-iteration instead of
 * silently counting a loop body exactly once.
 */
function findLoopBodyRanges(body: string): [number, number][] {
  const ranges: [number, number][] = [];
  const stack: { start: number; isLoop: boolean }[] = [];
  let pendingLoop = false;
  let i = 0;

  while (i < body.length) {
    const ch = body[i];
    if (ch === "{") {
      stack.push({ start: i, isLoop: pendingLoop });
      pendingLoop = false;
    } else if (ch === "}") {
      const frame = stack.pop();
      if (frame?.isLoop) ranges.push([frame.start, i]);
    } else if (ch === ";") {
      pendingLoop = false;
    } else if (/[A-Za-z_]/.test(ch)) {
      let j = i;
      while (j < body.length && /[A-Za-z0-9_]/.test(body[j])) j++;
      const word = body.slice(i, j);
      if (word === "for" || word === "while" || word === "loop") pendingLoop = true;
      i = j;
      continue;
    }
    i++;
  }
  return ranges;
}

/** Normalise a key expression into a stable, comparable string. */
function normaliseKey(expr: string): string {
  const cleaned = expr
    .trim()
    .replace(/^&/, "")
    .replace(/^(mut\s+)/, "")
    .replace(/\s+/g, "")
    .replace(/::default\(\)$/, "")
    .replace(/::new\([^()]*\)$/, "");
  return cleaned.length > 80 ? `${cleaned.slice(0, 77)}…` : cleaned;
}

/**
 * Owning namespace of a storage key, e.g. `HotStorageKey` for
 * `HotStorageKey::DepositState` and `PauseDataKey` for
 * `pause::PauseDataKey::State(PauseType::Deposit)`.
 *
 * Returns `""` for variable keys (`&key`) and plain string literals, which have
 * no namespace and therefore no packing candidate.
 */
function keyEnumOf(key: string): string {
  const pair = /([A-Za-z_][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)/g;
  let found = "";
  for (let m = pair.exec(key); m; m = pair.exec(key)) {
    if (/^[A-Z]/.test(m[1]) && /^[A-Z]/.test(m[2])) found = m[1];
  }
  return found;
}

/** Split a call's argument list on its top-level commas. */
function splitArgs(args: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const ch of args) {
    if (ch === "(" || ch === "[" || ch === "{" || ch === "<") depth++;
    else if (ch === ")" || ch === "]" || ch === "}" || ch === ">") depth--;
    if (ch === "," && depth <= 0) {
      parts.push(current);
      current = "";
      continue;
    }
    current += ch;
  }
  if (current.trim()) parts.push(current);
  return parts;
}

/**
 * Best-effort resolution of a written value's `#[contracttype]` struct, so the
 * footprint byte estimate is grounded in the real declaration.
 */
function resolveWrittenStruct(value: string, ctx: {
  implStruct?: ContractStruct;
  locals: Map<string, string>;
  types: ContractTypeIndex;
}): ContractStruct | null {
  const expr = value.trim().replace(/^&/, "");

  // `&Foo { .. }` struct literal
  const literal = /^([A-Za-z_][A-Za-z0-9_]*)\s*\{/.exec(expr);
  if (literal) return ctx.types.structs.get(literal[1]) ?? null;

  // `&self.field` / `&self.field.field` inside `impl Foo`
  if (expr.startsWith("self")) {
    if (!ctx.implStruct) return null;
    let current: ContractStruct | null = ctx.implStruct;
    for (const seg of expr.slice("self".length).split(".").filter(Boolean)) {
      if (!current) return null;
      const field: { name: string; type: string } | undefined = current.fields.find((f) => f.name === seg);
      if (!field) return null;
      current = ctx.types.structs.get(field.type.trim()) ?? null;
    }
    return current;
  }

  // `&local` / `&local.field` where the binding has a declared struct type
  const parts = expr.split(".");
  const rootType = ctx.locals.get(parts[0]);
  if (rootType) {
    let current: ContractStruct | null = ctx.types.structs.get(rootType) ?? null;
    for (const seg of parts.slice(1)) {
      if (!current) return null;
      const field = current.fields.find((f) => f.name === seg);
      if (!field) return null;
      current = ctx.types.structs.get(field.type.trim()) ?? null;
    }
    return current;
  }

  return null;
}

/** A call site: either a free/qualified call or a method call on a value. */
interface CallRef {
  name: string;
  method: boolean;
}

interface BodyScan {
  accesses: StorageAccess[];
  crossContractCalls: CrossContractCall[];
  calls: CallRef[];
  hasLoop: boolean;
}

/** Find every storage access and external call inside a single function body. */
function scanBody(fn: RustFunction, types: ContractTypeIndex, variants: Set<string>): BodyScan {
  const body = fn.body;
  const aliases = storageAliases(body);
  const locals = typedLocals(body);
  for (const [name, type] of typedParams(fn.params)) if (!locals.has(name)) locals.set(name, type);
  const implStruct = fn.implType ? types.structs.get(fn.implType) : undefined;
  const loopRanges = findLoopBodyRanges(body);
  const inLoop = (offset: number): boolean =>
    loopRanges.some(([start, end]) => offset >= start && offset <= end);
  const accesses: StorageAccess[] = [];
  const crossContractCalls: CrossContractCall[] = [];
  const calls: CallRef[] = [];

  const pushAccess = (matchIndex: number, tier: StorageTier, method: string, argsRaw: string): void => {
    const args = splitArgs(argsRaw);
    if (args.length === 0) return;
    const key = normaliseKey(args[0]);
    const kind = KIND_OF[method];
    let entryBytes: number | null = null;
    if (kind === "write" && args.length > 1) {
      const struct = resolveWrittenStruct(args[1], { implStruct, locals, types });
      if (struct) entryBytes = estimateStructBytes(struct, types);
    }
    accesses.push({
      tier,
      kind,
      key,
      keyEnum: keyEnumOf(key),
      perIteration: inLoop(matchIndex),
      entryBytes,
      file: fn.file,
      line: fn.bodyLine + countNewlines(body.slice(0, matchIndex)),
    });
  };

  // Chained form: `<recv>.storage().<tier>().<op>(…)`, with an optional
  // turbofish (`storage.get::<_, T>(…)`).
  const chained = new RegExp(`(${IDENT})\\s*\\.\\s*storage\\s*\\(\\s*\\)\\s*\\.\\s*${TIER_RE}\\s*\\(\\s*\\)\\s*\\.\\s*(get|set|has|remove)\\s*(?:::\\s*<[^>]*>)?\\s*\\(`, "g");
  for (let m = chained.exec(body); m; m = chained.exec(body)) {
    const open = body.indexOf("(", m.index + m[0].length - 1);
    const close = matchDelimiter(body, open);
    pushAccess(m.index, TIER_OF[m[2]], m[3], body.slice(open + 1, close));
  }

  // Alias form: `storage.get(…)`
  for (const [receiver, tier] of aliases) {
    const aliasRe = new RegExp(`\\b${receiver}\\s*\\.\\s*(get|set|has|remove)\\s*(?:::\\s*<[^>]*>)?\\s*\\(`, "g");
    for (let m = aliasRe.exec(body); m; m = aliasRe.exec(body)) {
      const open = body.indexOf("(", m.index + m[0].length - 1);
      const close = matchDelimiter(body, open);
      pushAccess(m.index, tier, m[1], body.slice(open + 1, close));
    }
  }

  // External calls.
  const invoke = /\benv\s*\.\s*invoke_contract\s*(?:::\s*<[^>]*>)?\s*[(<]/g;
  for (let m = invoke.exec(body); m; m = invoke.exec(body)) {
    crossContractCalls.push({
      kind: "invoke_contract",
      file: fn.file,
      line: fn.bodyLine + countNewlines(body.slice(0, m.index)),
    });
  }
  const client = /\b(\w+)::Client\s*::\s*new\b/g;
  for (let m = client.exec(body); m; m = client.exec(body)) {
    crossContractCalls.push({
      kind: "token_client",
      file: fn.file,
      line: fn.bodyLine + countNewlines(body.slice(0, m.index)),
    });
  }

  // Internal calls: `name(..)` or `Type::name(..)` that are not method calls.
  // Enum-variant constructors (`Err(..)`, `PauseDataKey::State(..)`) and SDK
  // helpers are filtered out — they are not missing definitions, so listing them
  // as unresolved would drown the ones that matter.
  const isNotAFunction = (candidate: string): boolean => {
    const parts = candidate.split("::");
    const bare = parts[parts.length - 1];
    const owner = parts.length >= 2 ? parts[parts.length - 2] : "";
    if (SDK_NAMES.has(bare)) return true;
    if (owner && SDK_NAMES.has(owner)) return true;
    if (owner && PRIMITIVE_TYPES.has(owner)) return true;
    if (owner && variants.has(bare)) return true;
    return false;
  };

  const callRe = new RegExp(`(?:^|[^A-Za-z0-9_:.])([A-Za-z_][A-Za-z0-9_]*)\\s*\\(`, "g");
  for (let m = callRe.exec(body); m; m = callRe.exec(body)) {
    if (!isNotAFunction(m[1])) calls.push({ name: m[1], method: false });
  }

  // Path-qualified static calls: `DepositHotSlot::load(..)`, `borrow::deposit(..)`.
  const qualifiedRe = new RegExp(`(?:^|[^A-Za-z0-9_])([A-Za-z_][A-Za-z0-9_]*)\\s*::\\s*([A-Za-z_][A-Za-z0-9_]*)\\s*\\(`, "g");
  for (let m = qualifiedRe.exec(body); m; m = qualifiedRe.exec(body)) {
    const qualified = `${m[1]}::${m[2]}`;
    if (!isNotAFunction(qualified)) calls.push({ name: qualified, method: false });
  }

  // Method calls on a value: `hot.commit(env)`. These are only followed when the
  // name maps to exactly one definition in the whole tree — otherwise
  // `unwrap`/`clone`/`iter` and friends would flood the unresolved list with
  // SDK methods that were never candidates in the first place.
  const methodRe = new RegExp(`\\.\\s*([A-Za-z_][A-Za-z0-9_]*)\\s*(?:::<[^>]*>)?\\s*\\(`, "g");
  for (let m = methodRe.exec(body); m; m = methodRe.exec(body)) {
    if (SDK_NAMES.has(m[1]) || variants.has(m[1])) continue;
    calls.push({ name: m[1], method: true });
  }

  return { accesses, crossContractCalls, calls, hasLoop: loopRanges.length > 0 };
}

/** Every variant name declared by an indexed `#[contracttype]` / `#[contracterror]` enum. */
export function allEnumVariants(types: ContractTypeIndex): Set<string> {
  const out = new Set<string>();
  for (const def of types.enums.values()) {
    for (const variant of def.variants) out.add(variant);
  }
  return out;
}

function countNewlines(text: string): number {
  let n = 0;
  for (let i = 0; i < text.length; i++) if (text[i] === "\n") n++;
  return n;
}

export interface OperationFactsOptions extends IndexOptions {
  /** Restrict the facts to these entry point names. */
  operations?: string[];
}

/**
 * Resolve the storage behaviour of one contract entry point, following internal
 * calls transitively.
 */
export function resolveOperationFacts(
  index: ContractIndex,
  entryPoint: RustFunction,
  options: OperationFactsOptions = {},
): OperationFacts {
  const maxDepth = options.maxDepth ?? 8;
  const accesses: StorageAccess[] = [];
  const crossContractCalls: CrossContractCall[] = [];
  const callees: string[] = [];
  const unresolved: string[] = [];
  let hasLoop = false;
  const variants = allEnumVariants(index.types);

  // `impl Drop for Guard { fn drop(..) }` — its body runs on scope exit, so its
  // storage access is part of the cost even though nothing calls it.
  const dropImpls = new Map<string, RustFunction[]>();
  for (const fn of index.functions) {
    if (fn.implTrait !== "Drop" || fn.name !== "drop") continue;
    const list = dropImpls.get(fn.implType) ?? [];
    list.push(fn);
    dropImpls.set(fn.implType, list);
  }
  const dropTypesDone = new Set<string>();

  // `ancestors` is the current call path, not a global visited set: a helper
  // called from two places must be priced twice, while a genuine cycle must
  // terminate.
  const walk = (fn: RustFunction, depth: number, ancestors: Set<RustFunction>, via?: string): void => {
    const scan = scanBody(fn, index.types, variants);
    if (scan.hasLoop) hasLoop = true;
    for (const access of scan.accesses) {
      accesses.push(via ? { ...access, via } : access);
    }
    for (const call of scan.crossContractCalls) {
      crossContractCalls.push(via ? { ...call, via } : call);
    }
    if (depth >= maxDepth) return;
    const seenNames = new Set<string>();
    for (const call of scan.calls) {
      if (seenNames.has(call.name)) continue;
      seenNames.add(call.name);

      let matches: RustFunction[];
      let ambiguous = false;
      if (call.method) {
        // `x.foo(..)` can only reach a method, and only when the name maps to
        // exactly one `impl` in the tree. Without both conditions every
        // `.get(..)` in the code would be attributed to some unrelated
        // free function that happens to be named `get`.
        matches = index.functions.filter((c) => c.name === call.name && c.implType !== "");
        if (matches.length !== 1) continue;
      } else {
        const resolved = resolveCallee(index, call.name, fn.file);
        matches = resolved.matches;
        ambiguous = resolved.ambiguous;
      }

      if (matches.length === 0) {
        if (!unresolved.includes(call.name)) unresolved.push(call.name);
        continue;
      }
      if (ambiguous && !unresolved.includes(call.name)) unresolved.push(call.name);
      const target = matches[0];
      if (ancestors.has(target)) continue; // cycle
      const id = functionId(target);
      if (!callees.includes(id)) callees.push(id);
      const next = new Set(ancestors);
      next.add(target);
      walk(target, depth + 1, next, id);

      // RAII: a guard constructed inside this call runs its `Drop` body when the
      // value goes out of scope, even though nothing calls `drop` explicitly.
      if (target.implType) includeDropBody(target.implType, next, id);
    }
  };

  /** Merge the `Drop::drop` accesses for a guard type into the current trace. */
  const includeDropBody = (typeName: string, ancestors: Set<RustFunction>, via: string): void => {
    if (dropTypesDone.has(typeName)) return;
    dropTypesDone.add(typeName);
    for (const impl of dropImpls.get(typeName) ?? []) {
      const scan = scanBody(impl, index.types, variants);
      for (const access of scan.accesses) {
        accesses.push({ ...access, via: `${via} (Drop)` });
      }
      for (const call of scan.crossContractCalls) {
        crossContractCalls.push({ ...call, via: `${via} (Drop)` });
      }
    }
  };

  walk(entryPoint, 0, new Set([entryPoint]));

  return {
    operation: entryPoint.name,
    entryPoint: `${index.contract}::${entryPoint.name}`,
    file: entryPoint.file,
    line: entryPoint.line,
    accesses,
    crossContractCalls,
    callees,
    unresolvedCalls: unresolved,
    hasLoop,
  };
}

/** Resolve every entry point in the index, optionally filtered by name. */
export function resolveAllOperations(
  index: ContractIndex,
  options: OperationFactsOptions = {},
): OperationFacts[] {
  const wanted = options.operations ? new Set(options.operations) : null;
  return index.entryPoints
    .filter((fn) => !wanted || wanted.has(fn.name))
    .map((fn) => resolveOperationFacts(index, fn, options));
}
