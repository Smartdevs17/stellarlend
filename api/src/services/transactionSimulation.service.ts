import { createHash } from 'crypto';
import {
  Address,
  BASE_FEE,
  Contract,
  FeeBumpTransaction,
  StrKey,
  Transaction,
  TransactionBuilder,
  nativeToScVal,
  scValToNative,
  xdr,
} from '@stellar/stellar-sdk';
import { Server as SorobanServer } from '@stellar/stellar-sdk/rpc';
import { config } from '../config';
import { LendingOperation } from '../types';
import { ApiError, ErrorCode, ValidationError } from '../utils/errors';
import logger from '../utils/logger';
import { StellarService, CONTRACT_METHODS } from './stellar.service';
import { readCacheService } from './readCache.service';
import { summarizeSorobanData, SorobanResourceSummary } from './sorobanFees';

/**
 * Transaction simulation.
 *
 * Runs a user's transaction through Soroban RPC `simulateTransaction` and
 * returns a JSON-friendly view of the outcome: resource usage, minimum
 * resource fee, decoded return value, required authorizations, diagnostic
 * events, ledger state changes and any restore preamble. The caller supplies
 * either a base64 transaction envelope or a lending operation specification
 * that is built the same way the prepare endpoint builds it.
 *
 * Results are cached for SIMULATION_CACHE_TTL_MS keyed by the envelope hash,
 * so wallets that poll the same unsigned transaction reuse one RPC call.
 */

export const MAX_TRANSACTION_XDR_LENGTH = 20000;
export const LENDING_OPERATIONS: readonly LendingOperation[] = [
  'deposit',
  'borrow',
  'repay',
  'withdraw',
];

const TX_TIMEOUT_SECONDS = 300;

export interface OperationSimulationSpec {
  operation: LendingOperation;
  userAddress: string;
  amount: string;
  assetAddress?: string;
}

export interface XdrSimulationSpec {
  transactionXdr: string;
}

export type TransactionSimulationRequest = XdrSimulationSpec | OperationSimulationSpec;

export type SimulationStatus = 'success' | 'restore_required' | 'error';

export interface SimulationAuthEntry {
  xdr: string;
  credentials: 'source_account' | 'address';
  address?: string;
}

export interface SimulationStateChange {
  type: string;
  keyXdr: string;
}

export interface SimulationInvocationResult {
  retvalXdr: string;
  retval: unknown;
  auth: SimulationAuthEntry[];
}

export interface TransactionSimulationResult {
  success: boolean;
  status: SimulationStatus;
  transactionXdr: string;
  sourceAccount: string;
  operationCount: number;
  latestLedger: number;
  minResourceFee: string | null;
  resources: SorobanResourceSummary | null;
  memoryBytes: string | null;
  result: SimulationInvocationResult | null;
  events: string[];
  stateChanges: SimulationStateChange[];
  restorePreamble: { minResourceFee: string; resources: SorobanResourceSummary | null } | null;
  error: string | null;
  cached: boolean;
  simulatedAt: string;
}

/**
 * Validate a request body into a simulation request. Throws ValidationError
 * with a message that names the offending field.
 */
export function parseSimulationRequest(body: unknown): TransactionSimulationRequest {
  if (!body || typeof body !== 'object') {
    throw new ValidationError('Request body is required');
  }
  const input = body as Record<string, unknown>;

  if (input.transactionXdr !== undefined) {
    if (typeof input.transactionXdr !== 'string' || !input.transactionXdr.trim()) {
      throw new ValidationError('transactionXdr must be a non-empty string');
    }
    if (input.transactionXdr.length > MAX_TRANSACTION_XDR_LENGTH) {
      throw new ValidationError(
        `transactionXdr must be <= ${MAX_TRANSACTION_XDR_LENGTH} characters`
      );
    }
    return { transactionXdr: input.transactionXdr.trim() };
  }

  const { operation, userAddress, assetAddress, amount } = input;
  if (operation === undefined) {
    throw new ValidationError(
      'Provide transactionXdr or an operation specification (operation, userAddress, amount)'
    );
  }
  if (
    typeof operation !== 'string' ||
    !LENDING_OPERATIONS.includes(operation as LendingOperation)
  ) {
    throw new ValidationError(`operation must be one of: ${LENDING_OPERATIONS.join(', ')}`);
  }
  if (typeof userAddress !== 'string' || !StrKey.isValidEd25519PublicKey(userAddress)) {
    throw new ValidationError('userAddress must be a valid Stellar public key');
  }
  if (typeof amount !== 'string' || !/^\d+$/.test(amount) || BigInt(amount) <= 0n) {
    throw new ValidationError('amount must be a positive integer string (stroops)');
  }
  if (assetAddress !== undefined) {
    if (typeof assetAddress !== 'string' || !StrKey.isValidContract(assetAddress)) {
      throw new ValidationError('assetAddress must be a valid contract address');
    }
  }

  return {
    operation: operation as LendingOperation,
    userAddress,
    amount,
    ...(typeof assetAddress === 'string' ? { assetAddress } : {}),
  };
}

/** Convert simulation values (bigint, Buffer, Map) into JSON-serializable data. */
export function toJsonSafe(value: unknown): unknown {
  if (typeof value === 'bigint') {
    return value.toString();
  }
  if (value instanceof Uint8Array) {
    return Buffer.from(value).toString('base64');
  }
  if (value instanceof Map) {
    const out: Record<string, unknown> = {};
    for (const [k, v] of value) {
      out[String(k)] = toJsonSafe(v);
    }
    return out;
  }
  if (Array.isArray(value)) {
    return value.map((item) => toJsonSafe(item));
  }
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = toJsonSafe(v);
    }
    return out;
  }
  return value;
}

function toBase64List(items: unknown): string[] {
  if (!Array.isArray(items)) {
    return [];
  }
  return items.map((item) => {
    if (typeof item === 'string') return item;
    const encodable = item as { toXDR?: (format: 'base64') => string };
    return typeof encodable?.toXDR === 'function' ? encodable.toXDR('base64') : String(item);
  });
}

function describeAuthEntry(raw: unknown): SimulationAuthEntry {
  const entry =
    typeof raw === 'string'
      ? xdr.SorobanAuthorizationEntry.fromXDR(raw, 'base64')
      : (raw as xdr.SorobanAuthorizationEntry);
  const encoded = entry.toXDR('base64');
  try {
    const credentials = entry.credentials();
    if (credentials.switch().name === 'sorobanCredentialsSourceAccount') {
      return { xdr: encoded, credentials: 'source_account' };
    }
    const address = Address.fromScAddress(credentials.address().address()).toString();
    return { xdr: encoded, credentials: 'address', address };
  } catch {
    return { xdr: encoded, credentials: 'address' };
  }
}

function describeChangeType(type: unknown): string {
  if (type === 1) return 'created';
  if (type === 2) return 'updated';
  if (type === 3) return 'deleted';
  return String(type);
}

function decodeInvocationResult(sim: Record<string, any>): SimulationInvocationResult | null {
  const parsed = sim.result as { retval?: unknown; auth?: unknown[] } | undefined;
  const raw = Array.isArray(sim.results)
    ? (sim.results[0] as { xdr?: string; auth?: unknown[] })
    : undefined;
  const retvalRaw = parsed?.retval ?? raw?.xdr;
  if (!retvalRaw) {
    return null;
  }
  const scVal =
    typeof retvalRaw === 'string'
      ? xdr.ScVal.fromXDR(retvalRaw, 'base64')
      : (retvalRaw as xdr.ScVal);
  const authRaw = parsed?.auth ?? raw?.auth ?? [];
  return {
    retvalXdr: scVal.toXDR('base64'),
    retval: toJsonSafe(scValToNative(scVal)),
    auth: Array.isArray(authRaw) ? authRaw.map((entry) => describeAuthEntry(entry)) : [],
  };
}

/**
 * Normalize a Soroban RPC simulation response (parsed SDK shape or raw JSON
 * shape) into the API result. Pure: same response in, same result out.
 */
export function normalizeSimulation(
  simulation: unknown,
  tx: Transaction | FeeBumpTransaction,
  transactionXdr: string,
  simulatedAt: string = new Date().toISOString()
): TransactionSimulationResult {
  const sim = (simulation ?? {}) as Record<string, any>;
  const inner = tx instanceof FeeBumpTransaction ? tx.innerTransaction : tx;
  const base = {
    transactionXdr,
    sourceAccount: inner.source,
    operationCount: inner.operations.length,
    latestLedger: Number(sim.latestLedger ?? 0),
    events: toBase64List(sim.events),
    cached: false,
    simulatedAt,
  };

  if (sim.error) {
    return {
      ...base,
      success: false,
      status: 'error',
      minResourceFee: null,
      resources: null,
      memoryBytes: null,
      result: null,
      stateChanges: [],
      restorePreamble: null,
      error: String(sim.error),
    };
  }

  const restorePreamble = sim.restorePreamble
    ? {
        minResourceFee: String(sim.restorePreamble.minResourceFee ?? '0'),
        resources: summarizeSorobanData(sim.restorePreamble.transactionData),
      }
    : null;

  const stateChanges: SimulationStateChange[] = Array.isArray(sim.stateChanges)
    ? sim.stateChanges.map((change: { type: unknown; key: unknown }) => ({
        type: describeChangeType(change.type),
        keyXdr:
          typeof change.key === 'string'
            ? change.key
            : (change.key as xdr.LedgerKey).toXDR('base64'),
      }))
    : [];

  return {
    ...base,
    success: true,
    status: restorePreamble ? 'restore_required' : 'success',
    minResourceFee: sim.minResourceFee !== undefined ? String(sim.minResourceFee) : null,
    resources: summarizeSorobanData(sim.transactionData),
    memoryBytes: sim.cost?.memBytes !== undefined ? String(sim.cost.memBytes) : null,
    result: decodeInvocationResult(sim),
    stateChanges,
    restorePreamble,
    error: null,
  };
}

export class TransactionSimulationService {
  private readonly server: SorobanServer;

  constructor(server?: SorobanServer) {
    this.server = server ?? new SorobanServer(config.stellar.sorobanRpcUrl);
  }

  /** Parse a base64 envelope against the configured network passphrase. */
  parseTransaction(transactionXdr: string): Transaction | FeeBumpTransaction {
    try {
      return TransactionBuilder.fromXDR(transactionXdr, config.stellar.networkPassphrase);
    } catch {
      throw new ValidationError('transactionXdr is not a valid base64 transaction envelope');
    }
  }

  /** Build the unsigned lending transaction exactly as the prepare endpoint does. */
  async buildOperation(spec: OperationSimulationSpec): Promise<Transaction> {
    const stellarService = new StellarService();
    const account = await stellarService.getAccount(spec.userAddress);
    const contract = new Contract(config.stellar.contractId);
    const params = [
      new Address(spec.userAddress).toScVal(),
      spec.assetAddress ? new Address(spec.assetAddress).toScVal() : xdr.ScVal.scvVoid(),
      nativeToScVal(BigInt(spec.amount), { type: 'i128' }),
    ];
    return new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: config.stellar.networkPassphrase,
    })
      .addOperation(contract.call(CONTRACT_METHODS[spec.operation], ...params))
      .setTimeout(TX_TIMEOUT_SECONDS)
      .build();
  }

  async simulate(request: TransactionSimulationRequest): Promise<TransactionSimulationResult> {
    const tx =
      'transactionXdr' in request
        ? this.parseTransaction(request.transactionXdr)
        : await this.buildOperation(request);
    const transactionXdr = tx.toXDR();
    const cacheId = `tx:${createHash('sha256').update(transactionXdr).digest('hex')}`;

    let loadedNow = false;
    const result = await readCacheService.getOrLoad<TransactionSimulationResult>(
      'simulation',
      cacheId,
      config.cache.simulationTtlMs,
      async () => {
        loadedNow = true;
        return this.simulateTransaction(tx, transactionXdr);
      }
    );
    return { ...result, cached: !loadedNow };
  }

  private async simulateTransaction(
    tx: Transaction | FeeBumpTransaction,
    transactionXdr: string
  ): Promise<TransactionSimulationResult> {
    let simulation: unknown;
    try {
      simulation = await this.server.simulateTransaction(tx);
    } catch (error) {
      logger.error('Soroban RPC simulation request failed:', error);
      throw new ApiError(502, 'Soroban RPC simulation request failed', ErrorCode.NETWORK_ERROR);
    }
    return normalizeSimulation(simulation, tx, transactionXdr);
  }
}

export const transactionSimulationService = new TransactionSimulationService();
