/**
 * Low-level Rust source scanner for the lending gas cost estimator (#1011).
 *
 * Deliberately *not* a Rust parser: it does a comment/string-aware scan and a
 * brace/paren match, which is enough to recover the three things the estimator
 * needs — function bodies, `#[contracttype]` declarations and `use` aliases —
 * while staying fast and dependency-free.
 *
 * Everything is pure: the caller supplies file contents, so the whole scanner is
 * unit-testable without touching the filesystem.
 */

/** A function or method definition recovered from a source file. */
export interface RustFunction {
  name: string;
  file: string;
  line: number;
  /** Source text between the outer braces, comments already stripped. */
  body: string;
  /** Source text of the parameter list, comments already stripped. */
  params: string;
  /** 1-based line of the first body line. */
  bodyLine: number;
  /** Enclosing `impl` type name, e.g. `DepositHotSlot`, or `""` for free fns. */
  implType: string;
  /** Enclosing trait name for `impl Trait for Type`, e.g. `Drop`. */
  implTrait: string;
  /** True when the definition carries `pub` / `pub(crate)` visibility. */
  isPublic: boolean;
}

/** A `#[contracttype] pub struct` — used to size written ledger entries. */
export interface ContractStruct {
  name: string;
  file: string;
  line: number;
  fields: { name: string; type: string }[];
}

/** A `#[contracttype] pub enum` — used to group storage keys by namespace. */
export interface ContractEnum {
  name: string;
  file: string;
  line: number;
  variants: string[];
}

export interface ContractTypeIndex {
  structs: Map<string, ContractStruct>;
  enums: Map<string, ContractEnum>;
}

const IDENT = "[A-Za-z_][A-Za-z0-9_]*";

/**
 * Replace comment and literal *contents* with spaces, preserving newlines so
 * that byte offsets and line numbers stay valid for everything downstream.
 *
 * Handles nested block comments, raw strings (`r"…"`, `r#"…"#`) and lifetimes
 * (`'a`) so that a `//` inside a string or a `'` in a lifetime cannot flip the
 * scanner into comment mode.
 */
export function stripCommentsAndLiterals(source: string): string {
  const out = source.split("");
  const n = source.length;
  let i = 0;
  // Byte offsets that are "content" get blanked, so `offsetAt` still works.
  const blank = (from: number, to: number): void => {
    for (let k = from; k < to; k++) {
      if (out[k] !== "\n") out[k] = " ";
    }
  };

  while (i < n) {
    const c = source[i];
    const next = source[i + 1];

    // Line comment
    if (c === "/" && next === "/") {
      let j = i;
      while (j < n && source[j] !== "\n") j++;
      blank(i, j);
      i = j;
      continue;
    }

    // Block comment (Rust allows nesting)
    if (c === "/" && next === "*") {
      let depth = 0;
      let j = i;
      while (j < n) {
        if (source[j] === "/" && source[j + 1] === "*") {
          depth++;
          j += 2;
          continue;
        }
        if (source[j] === "*" && source[j + 1] === "/") {
          depth--;
          j += 2;
          if (depth === 0) break;
          continue;
        }
        j++;
      }
      blank(i, Math.min(j, n));
      i = j;
      continue;
    }

    // Raw string: r"…" / r#"…"# (any number of hashes)
    if (c === "r" && next !== undefined && (next === '"' || next === "#")) {
      let hashes = 0;
      let j = i + 1;
      while (source[j] === "#") {
        hashes++;
        j++;
      }
      if (source[j] === '"') {
        const terminator = `"${"#".repeat(hashes)}`;
        const end = source.indexOf(terminator, j + 1);
        const stop = end === -1 ? n : end + terminator.length;
        blank(i + 1, stop);
        i = stop;
        continue;
      }
    }

    // Normal string
    if (c === '"') {
      let j = i + 1;
      while (j < n) {
        if (source[j] === "\\") {
          j += 2;
          continue;
        }
        if (source[j] === '"') {
          j++;
          break;
        }
        j++;
      }
      blank(i, Math.min(j, n));
      i = j;
      continue;
    }

    // Char literal or lifetime: 'x' vs 'a
    if (c === "'") {
      const after = source[i + 1];
      const after2 = source[i + 2];
      const isCharLiteral =
        after !== undefined &&
        (after === "\\" || /[^A-Za-z0-9_]/.test(after) || (after2 !== undefined && after2 === "'"));
      if (isCharLiteral) {
        let j = i + 1;
        while (j < n) {
          if (source[j] === "\\") {
            j += 2;
            continue;
          }
          if (source[j] === "'") {
            j++;
            break;
          }
          j++;
        }
        blank(i, Math.min(j, n));
        i = j;
        continue;
      }
    }

    i++;
  }

  return out.join("");
}

/** 1-based line number of a character offset. */
export function lineAt(source: string, offset: number): number {
  let line = 1;
  for (let i = 0; i < offset && i < source.length; i++) {
    if (source[i] === "\n") line++;
  }
  return line;
}

/**
 * Index of the closing delimiter that matches the one at `open`.
 * Returns `source.length` when unbalanced.
 */
export function matchDelimiter(source: string, open: number): number {
  const pairs: Record<string, string> = { "{": "}", "(": ")", "[": "]" };
  const closeCh = pairs[source[open]];
  if (!closeCh) return source.length;
  let depth = 0;
  for (let i = open; i < source.length; i++) {
    const ch = source[i];
    if (ch === source[open]) depth++;
    else if (ch === closeCh) {
      depth--;
      if (depth === 0) return i;
    }
  }
  return source.length;
}

/**
 * Find every `fn` definition in a file, including methods inside `impl` blocks.
 * `#[contractfn]`-style attributes are skipped because the attribute name is
 * followed by `(`, not an identifier + `(`.
 */
export function indexFunctions(source: string, file: string): RustFunction[] {
  const src = stripCommentsAndLiterals(source);
  const out: RustFunction[] = [];
  // Offsets of `impl Type { ... }` and `impl Trait for Type { ... }` blocks,
  // used to tag methods with their owner.
  const impls: { typeName: string; traitName: string; start: number; end: number }[] = [];
  const implRe = new RegExp(
    `\\bimpl\\s+(?:<[^>]*>\\s*)?(?:(${IDENT})\\s+for\\s+)?(${IDENT})\\s*(?:<[^>]*>)?\\s*\\{`,
    "g",
  );
  for (let m = implRe.exec(src); m; m = implRe.exec(src)) {
    const brace = src.indexOf("{", m.index);
    impls.push({
      traitName: m[1] ?? "",
      typeName: m[2],
      start: m.index,
      end: matchDelimiter(src, brace),
    });
  }

  // A `fn` keyword is a definition when preceded by visibility or a boundary.
  const fnRe = new RegExp(`\\b(pub(?:\\([^)]*\\))?\\s+)?(?:default\\s+)?(?:const\\s+)?(?:async\\s+)?fn\\s+(${IDENT})\\s*(?:<[^>(]*>)?\\s*\\(`, "g");
  for (let m = fnRe.exec(src); m; m = fnRe.exec(src)) {
    const parenOpen = src.indexOf("(", m.index);
    const parenClose = matchDelimiter(src, parenOpen);
    // Return type (possibly multi-line) up to the body brace.
    let cursor = parenClose + 1;
    let brace = -1;
    while (cursor < src.length) {
      const ch = src[cursor];
      if (ch === ";") {
        brace = -1;
        break;
      }
      if (ch === "{") {
        brace = cursor;
        break;
      }
      cursor++;
    }
    if (brace === -1) continue;
    const braceClose = matchDelimiter(src, brace);
    const owner = impls.find((im) => m.index > im.start && brace < im.end);
    out.push({
      name: m[2],
      file,
      line: lineAt(src, m.index),
      body: src.slice(brace + 1, braceClose),
      params: src.slice(parenOpen + 1, parenClose),
      bodyLine: lineAt(src, brace + 1),
      implType: owner ? owner.typeName : "",
      implTrait: owner ? owner.traitName : "",
      isPublic: Boolean(m[1]),
    });
  }
  return out;
}

/**
 * Parse `#[contracttype]` structs and enums.
 *
 * These give the estimator two things: the *namespace* a storage key belongs to
 * (enum name, used for packing detection) and the serialized width of a written
 * value (struct fields, used for the footprint byte estimate).
 */
export function indexContractTypes(source: string, file: string): ContractTypeIndex {
  const src = stripCommentsAndLiterals(source);
  const structs = new Map<string, ContractStruct>();
  const enums = new Map<string, ContractEnum>();

  // Locate the attributes first, then find the declaration each one applies to.
  // Matching the declaration and the attribute in one regex would let the first
  // match swallow the second one's anchor, silently dropping types.
  const attrRe = /#\[(?:contracttype|contracterror)\]/g;
  const declRe = new RegExp(`\\b(struct|enum)\\s+(${IDENT})\\s*(?:\\([^)]*\\))?\\s*\\{`, "g");
  const attrs: number[] = [];
  for (let a = attrRe.exec(src); a; a = attrRe.exec(src)) attrs.push(a.index);

  attrs.forEach((attrIndex, position) => {
    const nextAttr = position + 1 < attrs.length ? attrs[position + 1] : src.length;
    declRe.lastIndex = attrIndex;
    const decl = declRe.exec(src);
    // The declaration must belong to *this* attribute: `#[derive(..)]` and
    // `#[allow(..)]` sit in between and are deliberately not anchors.
    if (!decl || decl.index >= nextAttr) return;

    const [keyword, name] = [decl[1], decl[2]];
    const brace = src.indexOf("{", decl.index);
    const end = matchDelimiter(src, brace);
    const body = src.slice(brace + 1, end);
    const line = lineAt(src, decl.index);

    if (keyword === "struct") {
      const fields = parseStructFields(body);
      if (!structs.has(name)) structs.set(name, { name, file, line, fields });
    } else {
      if (!enums.has(name)) enums.set(name, { name, file, line, variants: parseEnumVariants(body) });
    }
  });

  // Plain `pub struct` declarations (no attribute) still carry the field types
  // the estimator needs to size a written value: `hot.commit(..)` writes
  // `&self.state`, and `state`'s type is declared on the un-annotated
  // `DepositHotSlot` struct.
  const plainStructRe = new RegExp(`(?:^|[^A-Za-z0-9_])(?:pub(?:\\([^)]*\\))?\\s+)?struct\\s+(${IDENT})\\s*(?:<[^>]*>)?\\s*\\{`, "g");
  for (let m = plainStructRe.exec(src); m; m = plainStructRe.exec(src)) {
    if (structs.has(m[1])) continue;
    const brace = src.indexOf("{", m.index);
    const end = matchDelimiter(src, brace);
    structs.set(m[1], {
      name: m[1],
      file,
      line: lineAt(src, m.index),
      fields: parseStructFields(src.slice(brace + 1, end)),
    });
  }

  return { structs, enums };
}

/** `pub name: Type,` pairs inside a struct body. */
function parseStructFields(body: string): { name: string; type: string }[] {
  const fields: { name: string; type: string }[] = [];
  const fieldRe = new RegExp(`(?:^|[,;{}\\s])pub\\s+(${IDENT})\\s*:\\s*([^,;{}]+)`, "g");
  for (let f = fieldRe.exec(body); f; f = fieldRe.exec(body)) {
    fields.push({ name: f[1], type: f[2].trim() });
  }
  return fields;
}

/** Variant names of an enum body, with tuple payloads dropped. */
function parseEnumVariants(body: string): string[] {
  const variants: string[] = [];
  let depth = 0;
  let current = "";
  const flush = (): void => {
    const trimmed = current.trim();
    if (!trimmed) return;
    const variant = trimmed.split(/[\s({<]/, 1)[0];
    if (variant && /^[A-Za-z_][A-Za-z0-9_]*$/.test(variant)) variants.push(variant);
  };
  for (const ch of body) {
    if (ch === "(" || ch === "<" || ch === "[" || ch === "{") depth++;
    else if (ch === ")" || ch === "]" || ch === "}") depth--;
    if (ch === "," && depth <= 0) {
      flush();
      current = "";
      continue;
    }
    current += ch;
  }
  flush();
  return variants;
}

/**
 * Build an alias table from `use` statements so that calls like
 * `deposit_logic(&env, …)` resolve to `deposit::deposit`.
 *
 * Returns `alias -> "module::originalName"`.
 */
export function indexUseAliases(source: string): Map<string, string> {
  const src = stripCommentsAndLiterals(source);
  const aliases = new Map<string, string>();

  // `use a::b::{c, d as e};`  — brace groups
  const groupRe = new RegExp(`\\buse\\s+(${IDENT})(?:::(${IDENT}))?::\\{([^}]*)\\}\\s*;`, "g");
  for (let m = groupRe.exec(src); m; m = groupRe.exec(src)) {
    const module = m[2] ? `${m[1]}::${m[2]}` : m[1];
    for (const rawItem of m[3].split(",")) {
      const item = rawItem.trim();
      if (!item) continue;
      const asMatch = /^([A-Za-z_][A-Za-z0-9_]*)\s+as\s+([A-Za-z_][A-Za-z0-9_]*)$/.exec(item);
      if (asMatch) aliases.set(asMatch[2], `${module}::${asMatch[1]}`);
      else if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(item)) aliases.set(item, `${module}::${item}`);
    }
  }

  // `use a::b;` — single items
  const singleRe = new RegExp(`\\buse\\s+(${IDENT})::(${IDENT})::(${IDENT})\\s*;`, "g");
  for (let m = singleRe.exec(src); m; m = singleRe.exec(src)) {
    aliases.set(m[3], `${m[1]}::${m[2]}::${m[3]}`);
  }

  return aliases;
}

/**
 * Byte widths for the primitive types the lending pool persists.
 * Keyed by the Rust type name; `Address` is a `#[contracttype]` alias that
 * encodes as a 32-byte ScVal. Variable-width types (`String`, `Bytes`, `Vec`,
 * `Map`) are deliberately absent so they resolve to `null` rather than a guess.
 */
const PRIMITIVE_WIDTH: Record<string, number> = {
  bool: 1,
  Bool: 1,
  u8: 1,
  i8: 1,
  u16: 2,
  i16: 2,
  u32: 4,
  i32: 4,
  u64: 8,
  i64: 8,
  u128: 16,
  i128: 16,
  Address: 32,
};

/**
 * Estimate the serialized width of a `#[contracttype]` struct in bytes.
 *
 * Returns `null` when a field type is not a known primitive or a struct we have
 * already indexed — an honest "unknown" rather than a made-up number.
 */
export function estimateStructBytes(
  struct: ContractStruct,
  index: ContractTypeIndex,
  seen: Set<string> = new Set(),
): number | null {
  if (seen.has(struct.name)) return null; // recursive type — bail out
  seen.add(struct.name);
  let total = 0;
  for (const field of struct.fields) {
    const type = field.type.trim();
    if (PRIMITIVE_WIDTH[type] !== undefined) {
      total += PRIMITIVE_WIDTH[type];
      continue;
    }
    const nested = index.structs.get(type);
    if (nested) {
      const nestedBytes = estimateStructBytes(nested, index, new Set(seen));
      if (nestedBytes === null) return null;
      total += nestedBytes;
      continue;
    }
    return null; // String / Bytes / Vec / unknown
  }
  return total;
}
