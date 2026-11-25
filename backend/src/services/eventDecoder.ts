import { TextDecoder } from 'util';

export interface DecodedCompactEvent {
  schema_version: number;
  topic: string;
  module: 'pool' | 'risk' | 'liquidation';
  action:
    | 'deposit'
    | 'withdraw'
    | 'borrow'
    | 'repay'
    | 'liquidate'
    | 'risk_alert'
    | 'grace_period_warning'
    | 'config_updated';
  severity: 'info' | 'warning' | 'critical';
  actor: string;
  asset: string | null;
  counterparty: string | null;
  trace_id: string;
  timestamp: number;
  amount: string;
  health_factor_bps: string | null;
  payload: Record<string, string>;
}

const MODULES = ['pool', 'risk', 'liquidation'] as const;
const ACTIONS = [
  'deposit',
  'withdraw',
  'borrow',
  'repay',
  'liquidate',
  'risk_alert',
  'grace_period_warning',
  'config_updated',
] as const;
const SEVERITIES = ['info', 'warning', 'critical'] as const;
const MAX_U128 = (1n << 128n) - 1n;
const MAX_U64 = (1n << 64n) - 1n;

const TOPICS: Readonly<Record<string, string>> = Object.freeze({
  'pool.deposit': 'lend_deposit',
  'pool.withdraw': 'lend_withdraw',
  'pool.borrow': 'lend_borrow',
  'pool.repay': 'lend_repay',
  'pool.config_updated': 'cfg_updated',
  'risk.risk_alert': 'risk_alert',
  'risk.grace_period_warning': 'risk_grace_warn',
  'risk.config_updated': 'cfg_updated',
  'liquidation.liquidate': 'liq_execute',
  'liquidation.config_updated': 'cfg_updated',
});

class Reader {
  private position = 0;

  constructor(private readonly bytes: Uint8Array) {}

  readByte(): number {
    if (this.position >= this.bytes.length) throw new Error('Truncated compact event');
    return this.bytes[this.position++]!;
  }

  readUnsigned(): bigint {
    let value = 0n;
    for (let index = 0; index < 19; index += 1) {
      const byte = this.readByte();
      value |= BigInt(byte & 0x7f) << BigInt(index * 7);
      if ((byte & 0x80) === 0) {
        if (index > 0 && byte === 0) throw new Error('Non-canonical compact event varint');
        if (value > MAX_U128) throw new Error('Compact event integer exceeds i128 range');
        return value;
      }
    }
    throw new Error('Invalid compact event varint');
  }

  readSigned(): string {
    const encoded = this.readUnsigned();
    return ((encoded >> 1n) ^ -(encoded & 1n)).toString();
  }

  readString(): string {
    const length = this.readUnsigned();
    if (length > BigInt(this.bytes.length - this.position)) {
      throw new Error('Truncated compact event string');
    }
    const end = this.position + Number(length);
    const bytes = this.bytes.subarray(this.position, end);
    this.position = end;
    try {
      return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    } catch {
      throw new Error('Invalid UTF-8 in compact event');
    }
  }

  readOptionalString(): string | null {
    const present = this.readByte();
    if (present === 0) return null;
    if (present !== 1) throw new Error('Invalid compact event option tag');
    return this.readString();
  }

  readOptionalSigned(): string | null {
    const present = this.readByte();
    if (present === 0) return null;
    if (present !== 1) throw new Error('Invalid compact event option tag');
    return this.readSigned();
  }

  finished(): boolean {
    return this.position === this.bytes.length;
  }
}

function enumValue<T extends readonly string[]>(values: T, tag: number, name: string): T[number] {
  const value = values[tag];
  if (value === undefined) throw new Error(`Unknown compact event ${name} tag: ${tag}`);
  return value;
}

/** Decode the versioned compact byte format emitted by contracts/events. */
export function decodeCompactEvent(bytes: Uint8Array): DecodedCompactEvent {
  const reader = new Reader(bytes);
  const schemaVersion = reader.readByte();
  if (schemaVersion !== 1) throw new Error(`Unsupported compact event schema version: ${schemaVersion}`);

  const module = enumValue(MODULES, reader.readByte(), 'module');
  const action = enumValue(ACTIONS, reader.readByte(), 'action');
  const severity = enumValue(SEVERITIES, reader.readByte(), 'severity');
  const topic = TOPICS[`${module}.${action}`];
  if (!topic) throw new Error(`Unknown compact event topic: ${module}.${action}`);

  const actor = reader.readString();
  if (!actor) throw new Error('Compact event actor is empty');
  const asset = reader.readOptionalString();
  const counterparty = reader.readOptionalString();
  const traceId = reader.readString();
  if (!traceId) throw new Error('Compact event trace id is empty');

  const timestamp = reader.readUnsigned();
  if (timestamp === 0n || timestamp > MAX_U64 || timestamp > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error('Invalid compact event timestamp');
  }
  const amount = reader.readSigned();
  if (BigInt(amount) < 0n) throw new Error('Invalid compact event amount');
  const healthFactor = reader.readOptionalSigned();
  if (
    healthFactor !== null &&
    (BigInt(healthFactor) < 0n || BigInt(healthFactor) > 1_000_000n)
  ) {
    throw new Error('Invalid compact event health factor');
  }
  const payloadTag = reader.readByte();
  let payload: Record<string, string>;
  switch (payloadTag) {
    case 0:
      payload = { kind: 'pool', balance_after: reader.readSigned(), debt_after: reader.readSigned() };
      break;
    case 1:
      payload = {
        kind: 'risk',
        threshold_bps: reader.readSigned(),
        utilization_bps: reader.readSigned(),
      };
      break;
    case 2:
      payload = {
        kind: 'liquidation',
        debt_liquidated: reader.readSigned(),
        collateral_seized: reader.readSigned(),
        incentive_bps: reader.readSigned(),
      };
      break;
    case 3:
      payload = {
        kind: 'config',
        previous_value: reader.readSigned(),
        new_value: reader.readSigned(),
      };
      break;
    default:
      throw new Error(`Unknown compact event payload tag: ${payloadTag}`);
  }
  if (!reader.finished()) throw new Error('Unexpected trailing bytes in compact event');
  for (const [key, value] of Object.entries(payload)) {
    if (key !== 'kind' && BigInt(value) < 0n) {
      throw new Error(`Invalid compact event payload field: ${key}`);
    }
  }
  if (payload.kind === 'risk' && BigInt(payload.utilization_bps!) > 10_000n) {
    throw new Error('Invalid compact event utilization');
  }

  return {
    schema_version: schemaVersion,
    topic,
    module,
    action,
    severity,
    actor,
    asset,
    counterparty,
    trace_id: traceId,
    timestamp: Number(timestamp),
    amount,
    health_factor_bps: healthFactor,
    payload,
  };
}
