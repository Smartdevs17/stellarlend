/**
 * Runtime validator for LendingSnapshot objects.
 *
 * Zero external dependencies — no ajv, no zod.  Uses guard functions so
 * TypeScript narrows the type on a successful check.
 *
 * Returns a discriminated result rather than throwing so callers can decide
 * whether to hard-fail or fall back gracefully.
 */

import type {
  LendingSnapshot,
  LendingSnapshotV1,
  LendingSnapshotV2,
  PositionEntryV1,
  PoolEntryV1,
  GovernanceEntryV1,
} from './schema';

export type ValidationOk = { ok: true; snapshot: LendingSnapshot };
export type ValidationFail = { ok: false; errors: string[] };
export type ValidationResult = ValidationOk | ValidationFail;

// ── Low-level helpers ────────────────────────────────────────────────────────

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function isNonEmptyString(v: unknown): v is string {
  return typeof v === 'string' && v.length > 0;
}

function isPositiveInteger(v: unknown): v is number {
  return typeof v === 'number' && Number.isInteger(v) && v > 0;
}

function isNonNegativeInteger(v: unknown): v is number {
  return typeof v === 'number' && Number.isInteger(v) && v >= 0;
}

function isNumericString(v: unknown): v is string {
  if (typeof v !== 'string' || v.length === 0) return false;
  return /^-?\d+(\.\d+)?$/.test(v);
}

function isIso8601(v: unknown): v is string {
  if (typeof v !== 'string') return false;
  return !Number.isNaN(Date.parse(v));
}

// ── Entry validators ─────────────────────────────────────────────────────────

function validatePosition(entry: unknown, path: string, errors: string[]): boolean {
  if (!isObject(entry)) { errors.push(`${path}: must be an object`); return false; }
  let ok = true;
  if (!isNonEmptyString(entry['address'])) { errors.push(`${path}.address: must be a non-empty string`); ok = false; }
  if (!isNumericString(entry['collateral'])) { errors.push(`${path}.collateral: must be a numeric string`); ok = false; }
  if (!isNumericString(entry['debt'])) { errors.push(`${path}.debt: must be a numeric string`); ok = false; }
  if (!isNumericString(entry['healthFactor'])) { errors.push(`${path}.healthFactor: must be a numeric string`); ok = false; }
  if (
    'lastUpdatedLedger' in entry &&
    entry['lastUpdatedLedger'] !== undefined &&
    !isNonNegativeInteger(entry['lastUpdatedLedger'])
  ) {
    errors.push(`${path}.lastUpdatedLedger: must be a non-negative integer`);
    ok = false;
  }
  return ok;
}

function validatePool(entry: unknown, path: string, errors: string[]): boolean {
  if (!isObject(entry)) { errors.push(`${path}: must be an object`); return false; }
  let ok = true;
  if (!isNonEmptyString(entry['asset'])) { errors.push(`${path}.asset: must be a non-empty string`); ok = false; }
  if (!isPositiveInteger(entry['collateralFactorBps'])) { errors.push(`${path}.collateralFactorBps: must be a positive integer`); ok = false; }
  if (!isPositiveInteger(entry['interestRateBps'])) { errors.push(`${path}.interestRateBps: must be a positive integer`); ok = false; }
  if (!isNumericString(entry['supplyCap'])) { errors.push(`${path}.supplyCap: must be a numeric string`); ok = false; }
  if (
    'liquidationThresholdBps' in entry &&
    entry['liquidationThresholdBps'] !== undefined &&
    !isPositiveInteger(entry['liquidationThresholdBps'])
  ) {
    errors.push(`${path}.liquidationThresholdBps: must be a positive integer`);
    ok = false;
  }
  return ok;
}

function validateGovernance(entry: unknown, path: string, errors: string[]): boolean {
  if (!isObject(entry)) { errors.push(`${path}: must be an object`); return false; }
  let ok = true;
  if (!isPositiveInteger(entry['proposalId'])) { errors.push(`${path}.proposalId: must be a positive integer`); ok = false; }
  if (!isNonEmptyString(entry['status'])) { errors.push(`${path}.status: must be a non-empty string`); ok = false; }
  if (typeof entry['executed'] !== 'boolean') { errors.push(`${path}.executed: must be a boolean`); ok = false; }
  if (!isNumericString(entry['votesFor'])) { errors.push(`${path}.votesFor: must be a numeric string`); ok = false; }
  if (!isNumericString(entry['votesAgainst'])) { errors.push(`${path}.votesAgainst: must be a numeric string`); ok = false; }
  if (
    'timelockAt' in entry &&
    entry['timelockAt'] !== undefined &&
    !isNonNegativeInteger(entry['timelockAt'])
  ) {
    errors.push(`${path}.timelockAt: must be a non-negative integer`);
    ok = false;
  }
  return ok;
}

function validateArray<T>(
  arr: unknown,
  path: string,
  itemValidator: (item: unknown, p: string, e: string[]) => boolean,
  errors: string[],
): arr is T[] {
  if (!Array.isArray(arr)) { errors.push(`${path}: must be an array`); return false; }
  let ok = true;
  arr.forEach((item, i) => {
    if (!itemValidator(item, `${path}[${i}]`, errors)) ok = false;
  });
  return ok;
}

// ── Version-specific validators ──────────────────────────────────────────────

function validateV1(raw: Record<string, unknown>, errors: string[]): boolean {
  if (!isObject(raw['state'])) { errors.push('state: must be an object'); return false; }
  const state = raw['state'] as Record<string, unknown>;
  let ok = true;
  ok = validateArray(state['positions'], 'state.positions', validatePosition, errors) && ok;
  ok = validateArray(state['pools'], 'state.pools', validatePool, errors) && ok;
  ok = validateArray(state['governance'], 'state.governance', validateGovernance, errors) && ok;

  // Canonical format (issue #1618): known top-level keys only.
  const allowedTopLevel = new Set(['version', 'kind', 'recordedAt', 'state', 'seq']);
  for (const key of Object.keys(raw)) {
    if (!allowedTopLevel.has(key)) {
      errors.push(`${key}: unknown top-level field; snapshot format is version/kind/recordedAt/seq/state`);
      ok = false;
    }
  }
  return ok;
}

function validateV2(raw: Record<string, unknown>, errors: string[]): boolean {
  if (!isNonNegativeInteger(raw['seq'])) {
    errors.push('seq: must be a non-negative integer (required in v2)');
    return false;
  }
  return validateV1(raw, errors);
}

// ── Public API ───────────────────────────────────────────────────────────────

/**
 * Validate an unknown value as a LendingSnapshot.
 *
 * Collects all errors rather than failing on the first one, so callers can
 * log a complete picture of what is wrong.
 */
export function validateSnapshot(raw: unknown): ValidationResult {
  const errors: string[] = [];

  if (!isObject(raw)) {
    return { ok: false, errors: ['snapshot: must be a JSON object'] };
  }

  if (!isPositiveInteger(raw['version'])) {
    return { ok: false, errors: ['version: must be a positive integer'] };
  }

  if (raw['kind'] !== 'contract-state') {
    errors.push(`kind: expected "contract-state", got "${raw['kind']}"`);
  }

  if (!isIso8601(raw['recordedAt'])) {
    errors.push('recordedAt: must be a valid ISO-8601 date string');
  }

  const version = raw['version'] as number;
  let structureOk = false;

  switch (version) {
    case 1:
      structureOk = validateV1(raw, errors);
      break;
    case 2:
      structureOk = validateV2(raw, errors);
      break;
    default:
      errors.push(`version: unsupported version ${version}`);
  }

  if (!structureOk || errors.length > 0) {
    return { ok: false, errors };
  }

  return { ok: true, snapshot: raw as unknown as LendingSnapshot };
}
