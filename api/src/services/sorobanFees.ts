import { BASE_FEE, SorobanDataBuilder, xdr } from '@stellar/stellar-sdk';
import { ValidationError } from '../utils/errors';

/**
 * Deterministic fee arithmetic for Soroban transactions.
 *
 * A Soroban transaction pays two fees: the inclusion fee (base fee per
 * operation, competes for ledger space) and the resource fee returned by
 * simulation (CPU, memory, ledger reads and writes). Everything here is pure
 * BigInt math so the same inputs always yield the same estimate.
 */

export interface SorobanResourceSummary {
  /** CPU instructions the host will budget for the invocation. */
  cpuInstructions: string;
  /** Bytes read from ledger storage. */
  readBytes: string;
  /** Bytes written to ledger storage. */
  writeBytes: string;
  /** Number of ledger entries in the read-only footprint. */
  readOnlyEntries: number;
  /** Number of ledger entries in the read-write footprint. */
  readWriteEntries: number;
  /** Resource fee (stroops) embedded in the transaction data. */
  resourceFee: string;
}

export interface FeeEstimateInput {
  /** Minimum resource fee (stroops) reported by simulation. */
  minResourceFee: string;
  /** Number of operations in the envelope; each pays the inclusion fee. */
  operationCount: number;
  /** Safety margin added on top of the total, integer percent 0..100. */
  feeMarginPercent?: number;
  /** Inclusion fee per operation (stroops); defaults to the network base fee. */
  baseFee?: string;
}

export interface FeeEstimate {
  baseFee: string;
  operationCount: number;
  inclusionFee: string;
  resourceFee: string;
  totalFee: string;
  feeMarginPercent: number;
  recommendedFee: string;
}

export const DEFAULT_FEE_MARGIN_PERCENT = 10;
export const MAX_FEE_MARGIN_PERCENT = 100;

type SorobanDataInput = SorobanDataBuilder | xdr.SorobanTransactionData | string | null | undefined;

interface ResourceAccessors {
  instructions(): number;
  writeBytes(): number;
  diskReadBytes?(): number;
  readBytes?(): number;
  footprint(): xdr.LedgerFootprint;
}

function toTransactionData(data: SorobanDataInput): xdr.SorobanTransactionData | null {
  if (data === undefined || data === null || data === '') {
    return null;
  }
  if (typeof data === 'string') {
    return xdr.SorobanTransactionData.fromXDR(data, 'base64');
  }
  if (typeof (data as SorobanDataBuilder).build === 'function') {
    return (data as SorobanDataBuilder).build();
  }
  return data as xdr.SorobanTransactionData;
}

/**
 * Summarize the resources declared in a SorobanTransactionData blob.
 * Accepts the SDK builder, the raw XDR object, or a base64 string.
 * Returns null when no transaction data is present.
 */
export function summarizeSorobanData(data: SorobanDataInput): SorobanResourceSummary | null {
  const built = toTransactionData(data);
  if (!built) {
    return null;
  }

  const resources = built.resources() as unknown as ResourceAccessors;
  const footprint = resources.footprint();
  const readBytes =
    typeof resources.diskReadBytes === 'function'
      ? resources.diskReadBytes()
      : typeof resources.readBytes === 'function'
        ? resources.readBytes()
        : 0;

  return {
    cpuInstructions: String(resources.instructions()),
    readBytes: String(readBytes),
    writeBytes: String(resources.writeBytes()),
    readOnlyEntries: footprint.readOnly().length,
    readWriteEntries: footprint.readWrite().length,
    resourceFee: built.resourceFee().toString(),
  };
}

function toNonNegativeBigInt(value: string | number | bigint, field: string): bigint {
  let parsed: bigint;
  try {
    if (typeof value === 'string' && !/^\d+$/.test(value.trim())) {
      throw new Error('not an integer string');
    }
    parsed = BigInt(value);
  } catch {
    throw new ValidationError(`${field} must be a non-negative integer`);
  }
  if (parsed < 0n) {
    throw new ValidationError(`${field} must be a non-negative integer`);
  }
  return parsed;
}

/**
 * Validate a user-supplied fee margin. Undefined falls back to the default.
 */
export function parseFeeMarginPercent(raw: unknown): number {
  if (raw === undefined || raw === null || raw === '') {
    return DEFAULT_FEE_MARGIN_PERCENT;
  }
  const value = typeof raw === 'string' ? Number(raw) : raw;
  if (
    typeof value !== 'number' ||
    !Number.isInteger(value) ||
    value < 0 ||
    value > MAX_FEE_MARGIN_PERCENT
  ) {
    throw new ValidationError(
      `feeMarginPercent must be an integer between 0 and ${MAX_FEE_MARGIN_PERCENT}`
    );
  }
  return value;
}

/**
 * Compute inclusion, resource, total and recommended fees in stroops.
 * The recommended fee rounds the margin up so it never under-pays.
 */
export function estimateFees(input: FeeEstimateInput): FeeEstimate {
  if (!Number.isInteger(input.operationCount) || input.operationCount < 1) {
    throw new ValidationError('operationCount must be a positive integer');
  }
  const margin = parseFeeMarginPercent(input.feeMarginPercent);
  const baseFee = toNonNegativeBigInt(input.baseFee ?? BASE_FEE, 'baseFee');
  const resourceFee = toNonNegativeBigInt(input.minResourceFee, 'minResourceFee');
  const operations = BigInt(input.operationCount);

  const inclusionFee = baseFee * operations;
  const totalFee = inclusionFee + resourceFee;
  const marginFee = (totalFee * BigInt(margin) + 99n) / 100n;
  const recommendedFee = totalFee + marginFee;

  return {
    baseFee: baseFee.toString(),
    operationCount: input.operationCount,
    inclusionFee: inclusionFee.toString(),
    resourceFee: resourceFee.toString(),
    totalFee: totalFee.toString(),
    feeMarginPercent: margin,
    recommendedFee: recommendedFee.toString(),
  };
}
