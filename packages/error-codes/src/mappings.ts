import {
  ContractDomain,
  getProtocolErrorMetadata,
  isProtocolErrorCode,
  ProtocolErrorCode,
} from './codes.ts';
import type { ProtocolErrorMetadata } from './codes.ts';
import { emitErrorTelemetry } from './observability.ts';

export const CONTRACT_ERROR_MAPPINGS: Record<ContractDomain, Record<number, ProtocolErrorCode>> = {
  [ContractDomain.Core]: {},
  [ContractDomain.Attestation]: {
    1: ProtocolErrorCode.Unauthorized,
    2: ProtocolErrorCode.AttestationExpired,
    3: ProtocolErrorCode.InvalidSignature,
  },
  [ContractDomain.Events]: {
    1: ProtocolErrorCode.InvalidAmount,
  },
  [ContractDomain.FlashLoan]: {
    1: ProtocolErrorCode.Unauthorized,
    2: ProtocolErrorCode.Unauthorized,
    3: ProtocolErrorCode.CrossContractFailure,
    4: ProtocolErrorCode.FlashLoanRepaymentFailed,
    5: ProtocolErrorCode.InvalidConfiguration,
  },
  [ContractDomain.Health]: {
    1: ProtocolErrorCode.HealthFactorTooLow,
    2: ProtocolErrorCode.InvalidAmount,
  },
  [ContractDomain.Interest]: {
    1: ProtocolErrorCode.InvalidConfiguration,
    2: ProtocolErrorCode.DuplicateRequest,
    3: ProtocolErrorCode.Overflow,
  },
  [ContractDomain.Keeper]: {
    1: ProtocolErrorCode.Unauthorized,
    2: ProtocolErrorCode.NotInitialized,
    3: ProtocolErrorCode.AlreadyInitialized,
    4: ProtocolErrorCode.InvalidConfiguration,
    5: ProtocolErrorCode.NotFound,
    6: ProtocolErrorCode.KeeperSlashed,
    7: ProtocolErrorCode.InsufficientBalance,
    8: ProtocolErrorCode.InvalidAmount,
    9: ProtocolErrorCode.InvalidConfiguration,
  },
  [ContractDomain.Migration]: {
    1: ProtocolErrorCode.InvalidAddress,
    2: ProtocolErrorCode.InvalidAddress,
    3: ProtocolErrorCode.InsufficientCollateral,
    4: ProtocolErrorCode.SlippageExceeded,
    5: ProtocolErrorCode.HealthFactorTooLow,
  },
  [ContractDomain.MockOracle]: {
    1: ProtocolErrorCode.OracleUnavailable,
    2: ProtocolErrorCode.NotFound,
  },
  [ContractDomain.Oracle]: {
    1: ProtocolErrorCode.OracleStale,
    2: ProtocolErrorCode.InvalidAmount,
    3: ProtocolErrorCode.InvalidState,
    4: ProtocolErrorCode.OracleUnavailable,
  },
  [ContractDomain.Params]: {
    1: ProtocolErrorCode.InvalidConfiguration,
    2: ProtocolErrorCode.LimitExceeded,
  },
  [ContractDomain.Pause]: {
    1: ProtocolErrorCode.Unauthorized,
    2: ProtocolErrorCode.TimelockActive,
    3: ProtocolErrorCode.InvalidSignature,
    4: ProtocolErrorCode.NotFound,
    5: ProtocolErrorCode.InvalidState,
  },
  [ContractDomain.Pool]: {
    1: ProtocolErrorCode.InvalidConfiguration,
    2: ProtocolErrorCode.InvalidState,
    3: ProtocolErrorCode.PauseActive,
  },
  [ContractDomain.Queue]: {
    1: ProtocolErrorCode.CooldownNotMet,
    2: ProtocolErrorCode.LimitExceeded,
    3: ProtocolErrorCode.InvalidState,
    4: ProtocolErrorCode.InvalidAmount,
  },
  [ContractDomain.SafeMath]: {
    1: ProtocolErrorCode.Overflow,
    2: ProtocolErrorCode.Underflow,
    3: ProtocolErrorCode.InvalidAmount,
  },
  [ContractDomain.Timelock]: {
    1: ProtocolErrorCode.AlreadyInitialized,
    2: ProtocolErrorCode.NotInitialized,
    3: ProtocolErrorCode.Unauthorized,
    4: ProtocolErrorCode.NotFound,
    5: ProtocolErrorCode.InvalidConfiguration,
    6: ProtocolErrorCode.InvalidConfiguration,
    7: ProtocolErrorCode.TimelockActive,
    8: ProtocolErrorCode.InvalidState,
    9: ProtocolErrorCode.InvalidState,
    10: ProtocolErrorCode.InvalidConfiguration,
    11: ProtocolErrorCode.InvalidSignature,
  },
};

export const BACKEND_ERROR_MAPPINGS: Record<string, ProtocolErrorCode> = {
  UNAUTHORIZED: ProtocolErrorCode.Unauthorized,
  UNAUTHORIZED_REVIEWER: ProtocolErrorCode.Unauthorized,
  ALREADY_INITIALIZED: ProtocolErrorCode.AlreadyInitialized,
  NOT_INITIALIZED: ProtocolErrorCode.NotInitialized,
  INVALID_ADDRESS: ProtocolErrorCode.InvalidAddress,
  INVALID_AMOUNT: ProtocolErrorCode.InvalidAmount,
  INVALID_SUBMISSION: ProtocolErrorCode.InvalidConfiguration,
  INVALID_DECISION: ProtocolErrorCode.InvalidConfiguration,
  INVALID_REASON: ProtocolErrorCode.InvalidConfiguration,
  INVALID_CONFIGURATION: ProtocolErrorCode.InvalidConfiguration,
  INVALID_STATE: ProtocolErrorCode.InvalidState,
  ALREADY_REVIEWED: ProtocolErrorCode.DuplicateRequest,
  DUPLICATE_REQUEST: ProtocolErrorCode.DuplicateRequest,
  INSUFFICIENT_BALANCE: ProtocolErrorCode.InsufficientBalance,
  INSUFFICIENT_COLLATERAL: ProtocolErrorCode.InsufficientCollateral,
  LIMIT_EXCEEDED: ProtocolErrorCode.LimitExceeded,
  REENTRANCY_DETECTED: ProtocolErrorCode.ReentrancyDetected,
  CROSS_CONTRACT_FAILURE: ProtocolErrorCode.CrossContractFailure,
  SLASH_FAILED: ProtocolErrorCode.CrossContractFailure,
  INVALID_SLASH_OUTCOME: ProtocolErrorCode.InvalidState,
  SERIALIZATION_FAILURE: ProtocolErrorCode.SerializationFailure,
  OVERFLOW: ProtocolErrorCode.Overflow,
  UNDERFLOW: ProtocolErrorCode.Underflow,
  UNSUPPORTED_OPERATION: ProtocolErrorCode.UnsupportedOperation,
  NOT_FOUND: ProtocolErrorCode.NotFound,
  DEADLINE_EXPIRED: ProtocolErrorCode.DeadlineExpired,
  ORACLE_STALE: ProtocolErrorCode.OracleStale,
  ORACLE_UNAVAILABLE: ProtocolErrorCode.OracleUnavailable,
  TIMELOCK_ACTIVE: ProtocolErrorCode.TimelockActive,
  PAUSE_ACTIVE: ProtocolErrorCode.PauseActive,
  SLIPPAGE_EXCEEDED: ProtocolErrorCode.SlippageExceeded,
  HEALTH_FACTOR_TOO_LOW: ProtocolErrorCode.HealthFactorTooLow,
  COOLDOWN_NOT_MET: ProtocolErrorCode.CooldownNotMet,
  ATTESTATION_EXPIRED: ProtocolErrorCode.AttestationExpired,
  INVALID_SIGNATURE: ProtocolErrorCode.InvalidSignature,
  FLASH_LOAN_REPAYMENT_FAILED: ProtocolErrorCode.FlashLoanRepaymentFailed,
  KEEPER_SLASHED: ProtocolErrorCode.KeeperSlashed,
};

export class UnifiedProtocolError extends Error {
  public readonly code: ProtocolErrorCode;
  public readonly domain?: ContractDomain;
  public readonly localCode?: number;
  public readonly backendCode?: string;
  public readonly details?: unknown;

  constructor(
    code: ProtocolErrorCode,
    options: {
      message?: string;
      domain?: ContractDomain;
      localCode?: number;
      backendCode?: string;
      details?: unknown;
    } = {}
  ) {
    const meta = getProtocolErrorMetadata(code);
    super(options.message ?? meta.message);
    this.name = 'UnifiedProtocolError';
    this.code = code;
    this.domain = options.domain;
    this.localCode = options.localCode;
    this.backendCode = options.backendCode;
    this.details = options.details;
    Object.setPrototypeOf(this, UnifiedProtocolError.prototype);
  }

  get metadata(): ProtocolErrorMetadata {
    return getProtocolErrorMetadata(this.code);
  }

  get slug(): string {
    return this.metadata.slug;
  }

  get httpStatus(): number {
    return this.metadata.httpStatus;
  }

  get retryable(): boolean {
    return this.metadata.retryable;
  }

  toJSON(): {
    code: ProtocolErrorCode;
    slug: string;
    name: string;
    httpStatus: number;
    retryable: boolean;
    message: string;
    handlingStrategy: string;
    domain?: ContractDomain;
    localCode?: number;
    backendCode?: string;
  } {
    const meta = this.metadata;
    return {
      code: meta.code,
      slug: meta.slug,
      name: meta.name,
      httpStatus: meta.httpStatus,
      retryable: meta.retryable,
      message: this.message,
      handlingStrategy: meta.handlingStrategy,
      domain: this.domain,
      localCode: this.localCode,
      backendCode: this.backendCode,
    };
  }
}

export function mapContractError(domain: ContractDomain, localCode: number): ProtocolErrorCode {
  if (domain === ContractDomain.Core) {
    return isProtocolErrorCode(localCode) ? localCode : ProtocolErrorCode.Unknown;
  }
  const table = CONTRACT_ERROR_MAPPINGS[domain];
  return table[localCode] ?? ProtocolErrorCode.Unknown;
}

export function mapBackendErrorCode(code: string): ProtocolErrorCode {
  const normalized = code.trim().toUpperCase();
  return BACKEND_ERROR_MAPPINGS[normalized] ?? ProtocolErrorCode.Unknown;
}

function extractLocalContractCode(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isInteger(value)) {
    return value;
  }
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (/^\d+$/.test(trimmed)) {
      return Number(trimmed);
    }
    const sorobanMatch = trimmed.match(/Error\s*\(\s*Contract\s*,\s*#?(\d+)\s*\)/i);
    if (sorobanMatch) {
      return Number(sorobanMatch[1]);
    }
    const hashMatch = trimmed.match(/#(\d{1,5})\b/);
    if (hashMatch) {
      return Number(hashMatch[1]);
    }
  }
  if (typeof value === 'object' && value !== null) {
    const obj = value as Record<string, unknown>;
    for (const key of ['code', 'errorCode', 'error_code', 'statusCode', 'message']) {
      if (typeof obj[key] === 'number' || typeof obj[key] === 'string') {
        const found = extractLocalContractCode(obj[key]);
        if (found !== undefined) {
          return found;
        }
      }
    }
  }
  return undefined;
}

function extractBackendStringCode(value: unknown): string | undefined {
  if (typeof value === 'string') {
    const trimmed = value.trim().toUpperCase();
    if (Object.prototype.hasOwnProperty.call(BACKEND_ERROR_MAPPINGS, trimmed)) {
      return trimmed;
    }
    return undefined;
  }
  if (typeof value === 'object' && value !== null) {
    const obj = value as Record<string, unknown>;
    if (typeof obj.code === 'string') {
      return extractBackendStringCode(obj.code);
    }
  }
  return undefined;
}

export function normalizeError(
  error: unknown,
  options: {
    domain?: ContractDomain;
    traceId?: string;
    tags?: Record<string, string | number | boolean>;
  } = {}
): UnifiedProtocolError {
  if (error instanceof UnifiedProtocolError) {
    return error;
  }

  let resolvedCode: ProtocolErrorCode = ProtocolErrorCode.Unknown;
  let localCode: number | undefined;
  let backendCode: string | undefined;

  backendCode = extractBackendStringCode(error);
  if (backendCode) {
    resolvedCode = mapBackendErrorCode(backendCode);
  } else {
    const extracted = extractLocalContractCode(error);
    if (extracted !== undefined) {
      if (options.domain && options.domain !== ContractDomain.Core) {
        localCode = extracted;
        resolvedCode = mapContractError(options.domain, extracted);
      } else if (isProtocolErrorCode(extracted)) {
        resolvedCode = extracted;
      }
    }
  }

  const unified = new UnifiedProtocolError(resolvedCode, {
    domain: options.domain,
    localCode,
    backendCode,
    details: error,
  });

  emitErrorTelemetry({
    code: unified.code,
    slug: unified.slug,
    httpStatus: unified.httpStatus,
    retryable: unified.retryable,
    domain: options.domain,
    localCode,
    backendCode,
    traceId: options.traceId,
    tags: options.tags,
  });

  return unified;
}
