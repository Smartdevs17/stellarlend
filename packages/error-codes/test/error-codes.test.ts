import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {
  ContractDomain,
  PROTOCOL_ERROR_CATALOG,
  ProtocolErrorCode,
  UnifiedProtocolError,
  getProtocolErrorMetadata,
  isProtocolErrorCode,
  mapBackendErrorCode,
  mapContractError,
  normalizeError,
  setErrorObserver,
} from '../src/index.ts';
import type { ErrorTelemetryEvent } from '../src/index.ts';

test('all ProtocolErrorCode values have unique numeric codes, slugs, and metadata', () => {
  const codes = Object.values(ProtocolErrorCode);
  const seenCodes = new Set<number>();
  const seenSlugs = new Set<string>();

  for (const code of codes) {
    assert.equal(isProtocolErrorCode(code), true);
    assert.equal(seenCodes.has(code), false, `Duplicate numeric code: ${code}`);
    seenCodes.add(code);

    const meta = getProtocolErrorMetadata(code);
    assert.equal(meta.code, code);
    assert.ok(meta.slug.length > 0);
    assert.equal(seenSlugs.has(meta.slug), false, `Duplicate slug: ${meta.slug}`);
    seenSlugs.add(meta.slug);
    assert.ok(meta.httpStatus >= 400);
    assert.ok(meta.message.length > 0);
    assert.ok(meta.handlingStrategy.length > 0);
  }

  assert.equal(isProtocolErrorCode(9999), false);
  assert.equal(getProtocolErrorMetadata(9999).code, ProtocolErrorCode.Unknown);
});

test('Rust contracts/errors/src/lib.rs stays 1:1 synchronized with TypeScript ProtocolErrorCode', () => {
  const rustPath = path.resolve(import.meta.dirname, '../../../contracts/errors/src/lib.rs');
  const rustSrc = fs.readFileSync(rustPath, 'utf8');

  for (const [name, code] of Object.entries(ProtocolErrorCode)) {
    const pattern = new RegExp(`\\b${name}\\s*=\\s*${code}\\b`);
    assert.match(rustSrc, pattern, `Rust ProtocolErrorCode missing or mismatched for ${name} = ${code}`);
  }

  assert.equal(Object.keys(PROTOCOL_ERROR_CATALOG).length, Object.keys(ProtocolErrorCode).length);
});

test('maps contract-local discriminants per ContractDomain accurately', () => {
  assert.equal(mapContractError(ContractDomain.Attestation, 2), ProtocolErrorCode.AttestationExpired);
  assert.equal(mapContractError(ContractDomain.FlashLoan, 4), ProtocolErrorCode.FlashLoanRepaymentFailed);
  assert.equal(mapContractError(ContractDomain.Interest, 3), ProtocolErrorCode.Overflow);
  assert.equal(mapContractError(ContractDomain.Keeper, 6), ProtocolErrorCode.KeeperSlashed);
  assert.equal(mapContractError(ContractDomain.Migration, 4), ProtocolErrorCode.SlippageExceeded);
  assert.equal(mapContractError(ContractDomain.MockOracle, 1), ProtocolErrorCode.OracleUnavailable);
  assert.equal(mapContractError(ContractDomain.Oracle, 1), ProtocolErrorCode.OracleStale);
  assert.equal(mapContractError(ContractDomain.Pause, 2), ProtocolErrorCode.TimelockActive);
  assert.equal(mapContractError(ContractDomain.Queue, 1), ProtocolErrorCode.CooldownNotMet);
  assert.equal(mapContractError(ContractDomain.Timelock, 7), ProtocolErrorCode.TimelockActive);
  assert.equal(mapContractError(ContractDomain.Timelock, 999), ProtocolErrorCode.Unknown);
  assert.equal(mapContractError(ContractDomain.Core, 1008), ProtocolErrorCode.InsufficientCollateral);
});

test('maps backend string error codes including KeeperReviewErrorCode', () => {
  assert.equal(mapBackendErrorCode('UNAUTHORIZED_REVIEWER'), ProtocolErrorCode.Unauthorized);
  assert.equal(mapBackendErrorCode('ALREADY_REVIEWED'), ProtocolErrorCode.DuplicateRequest);
  assert.equal(mapBackendErrorCode('SLASH_FAILED'), ProtocolErrorCode.CrossContractFailure);
  assert.equal(mapBackendErrorCode('INVALID_SLASH_OUTCOME'), ProtocolErrorCode.InvalidState);
  assert.equal(mapBackendErrorCode('oracle_stale'), ProtocolErrorCode.OracleStale);
  assert.equal(mapBackendErrorCode('NON_EXISTENT_CODE'), ProtocolErrorCode.Unknown);
});

test('normalizeError parses Soroban host errors, backend errors, and emits observability events', () => {
  const events: ErrorTelemetryEvent[] = [];
  setErrorObserver({
    onErrorNormalized(event) {
      events.push(event);
    },
  });

  try {
    const sorobanErr = normalizeError(new Error('HostError: Error(Contract, #1)'), {
      domain: ContractDomain.Oracle,
      traceId: 'trace-123',
      tags: { network: 'testnet' },
    });
    assert.ok(sorobanErr instanceof UnifiedProtocolError);
    assert.equal(sorobanErr.code, ProtocolErrorCode.OracleStale);
    assert.equal(sorobanErr.slug, 'ORACLE_STALE');
    assert.equal(sorobanErr.httpStatus, 503);
    assert.equal(sorobanErr.retryable, true);
    assert.equal(sorobanErr.localCode, 1);

    const backendErr = normalizeError({ code: 'UNAUTHORIZED_REVIEWER', message: 'Denied' });
    assert.equal(backendErr.code, ProtocolErrorCode.Unauthorized);
    assert.equal(backendErr.httpStatus, 403);
    assert.equal(backendErr.retryable, false);

    const json = sorobanErr.toJSON();
    assert.equal(json.code, ProtocolErrorCode.OracleStale);
    assert.equal(json.domain, ContractDomain.Oracle);

    assert.equal(events.length, 2);
    assert.equal(events[0].traceId, 'trace-123');
    assert.equal(events[0].slug, 'ORACLE_STALE');
    assert.equal(events[1].backendCode, 'UNAUTHORIZED_REVIEWER');
  } finally {
    setErrorObserver(null);
  }
});
