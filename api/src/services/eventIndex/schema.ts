/**
 * Event normalization and schema versioning.
 *
 * Converts decoded Soroban contract events into {@link IndexedEvent}s and
 * migrates older payload versions forward so queries and analytics only ever
 * see the current schema.
 *
 * Versioning rules (mirrors `EVENT_SCHEMA_VERSION` in
 * `stellar-lend/contracts/hello-world/src/events.rs`):
 * - `proto_evt` envelopes carry an explicit `schema_version` field.
 * - Typed events predate the envelope and are implicitly version 1.
 * - When the contract bumps the version, register an upcaster for the old
 *   version in {@link DEFAULT_UPCASTERS} (`n` → `n + 1`); upcasters chain.
 * - Events from a newer, unknown version are stored verbatim and flagged
 *   `schemaStatus: 'unknown'` rather than dropped, so nothing is lost while
 *   the indexer is being upgraded.
 */

import type { EventEnvelope, IndexedEvent, RawContractEvent, SchemaStatus } from './types';

export const CURRENT_SCHEMA_VERSION = 1;

/** Upcaster: migrates a payload from version `n` to `n + 1`. */
export type Upcaster = (data: Record<string, unknown>) => Record<string, unknown>;

/** Keyed by the version the upcaster migrates *from*. */
export const DEFAULT_UPCASTERS: Readonly<Record<number, Upcaster>> = Object.freeze({});

const STRUCTURED_TOPIC = 'proto_evt';
const ADDRESS_RE = /^[GC][A-Z2-7]{55}$/;

/** Render any decoded ScVal-native value as JSON-safe data. */
export function toJsonSafe(value: unknown): unknown {
  if (typeof value === 'bigint') return value.toString();
  if (value === null || value === undefined) return value ?? null;
  if (typeof value !== 'object') return value;
  if (value instanceof Uint8Array) return Buffer.from(value).toString('hex');
  if (Array.isArray(value)) return value.map(toJsonSafe);
  if (value instanceof Map) {
    const out: Record<string, unknown> = {};
    for (const [k, v] of value) out[String(k)] = toJsonSafe(v);
    return out;
  }
  // Address / Contract objects expose a meaningful toString().
  const proto = Object.getPrototypeOf(value);
  if (
    proto !== Object.prototype &&
    proto !== null &&
    typeof (value as object).toString === 'function'
  ) {
    const s = String(value);
    if (s !== '[object Object]') return s;
  }
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(value as Record<string, unknown>)) out[k] = toJsonSafe(v);
  return out;
}

/** `FlashLoan` / `["FlashLoan"]` (unit enum variant) → `flash_loan`. */
function enumName(value: unknown): string | undefined {
  const raw = Array.isArray(value) && value.length === 1 ? value[0] : value;
  if (typeof raw !== 'string') return undefined;
  return raw
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .replace(/-/g, '_')
    .toLowerCase();
}

/** Typed events publish their snake_case struct name, e.g. `deposit_event`. */
function typeFromTopic(first: unknown): string {
  if (typeof first !== 'string' || !first) return 'unknown';
  return first.replace(/_event$/, '');
}

function collectAccounts(value: unknown, into: Set<string>, depth = 0): void {
  if (depth > 4 || value === null || value === undefined) return;
  if (typeof value === 'string') {
    if (ADDRESS_RE.test(value)) into.add(value);
    return;
  }
  if (Array.isArray(value)) {
    for (const v of value) collectAccounts(v, into, depth + 1);
    return;
  }
  if (typeof value === 'object') {
    for (const v of Object.values(value as Record<string, unknown>)) {
      collectAccounts(v, into, depth + 1);
    }
  }
}

function pickString(data: Record<string, unknown>, keys: string[]): string | undefined {
  for (const k of keys) {
    const v = data[k];
    if (typeof v === 'string' && v !== '') return v;
  }
  return undefined;
}

/**
 * Migrate `data` from `version` to {@link CURRENT_SCHEMA_VERSION}.
 * Returns the migrated payload, resulting version and status.
 */
export function upcast(
  data: Record<string, unknown>,
  version: number,
  upcasters: Readonly<Record<number, Upcaster>> = DEFAULT_UPCASTERS
): { data: Record<string, unknown>; version: number; status: SchemaStatus } {
  if (version > CURRENT_SCHEMA_VERSION) return { data, version, status: 'unknown' };
  let current = data;
  let v = version;
  while (v < CURRENT_SCHEMA_VERSION) {
    const step = upcasters[v];
    // A gap in the chain means we cannot safely interpret the payload.
    if (!step) return { data: current, version: v, status: 'unknown' };
    current = step(current);
    v += 1;
  }
  return { data: current, version: v, status: version === v ? 'current' : 'upcast' };
}

/** Normalize one decoded RPC event into the indexed representation. */
export function normalizeEvent(
  raw: RawContractEvent,
  upcasters: Readonly<Record<number, Upcaster>> = DEFAULT_UPCASTERS
): IndexedEvent {
  const topic = (raw.topic ?? []).map(toJsonSafe);
  const decoded = toJsonSafe(raw.value);
  const payload: Record<string, unknown> =
    decoded && typeof decoded === 'object' && !Array.isArray(decoded)
      ? (decoded as Record<string, unknown>)
      : { value: decoded };

  const isStructured = topic[0] === STRUCTURED_TOPIC;
  const envelope: EventEnvelope = isStructured ? 'structured_event_v1' : 'typed';
  const declaredVersion = isStructured ? Number(payload.schema_version ?? 1) : 1;
  const migrated = upcast(payload, declaredVersion, upcasters);
  const data = migrated.data;

  const accounts = new Set<string>();
  collectAccounts(topic.slice(1), accounts);
  collectAccounts(data, accounts);
  const contract = raw.contractId ? String(raw.contractId) : '';
  accounts.delete(contract);

  const event: IndexedEvent = {
    id: raw.id,
    type: typeFromTopic(topic[0]),
    contract,
    topic,
    data,
    timestamp: Date.parse(raw.ledgerClosedAt) || 0,
    ledger: raw.ledger,
    txHash: raw.txHash,
    accounts: [...accounts].sort(),
    envelope,
    schemaVersion: migrated.version,
    schemaStatus: migrated.status,
  };

  const isAddress = (v: unknown): v is string => typeof v === 'string' && ADDRESS_RE.test(v);
  if (isStructured) {
    // Topic layout: ("proto_evt", module, action, actor)
    event.module = enumName(topic[1]);
    event.action = (typeof data.action_name === 'string' && data.action_name) || enumName(topic[2]);
    if (isAddress(topic[3])) event.actor = topic[3];
    if (isAddress(data.counterparty)) event.counterparty = data.counterparty;
  } else {
    // Typed events: (name, ...#[topic] fields) — e.g. (deposit_event, user)
    // or (liquidation_event, liquidator, borrower).
    const topicAddresses = topic.slice(1).filter(isAddress);
    if (topicAddresses[0]) event.actor = topicAddresses[0];
    if (topicAddresses[1]) event.counterparty = topicAddresses[1];
  }

  const asset = pickString(data, ['asset', 'debt_asset', 'collateral_asset', 'token']);
  if (asset) event.asset = asset;
  const amount = data.amount ?? data.debt_liquidated ?? data.debt_amount ?? data.value;
  if (typeof amount === 'string' && /^-?\d+$/.test(amount)) event.amount = amount;
  else if (typeof amount === 'number' && Number.isInteger(amount)) event.amount = String(amount);

  return event;
}
