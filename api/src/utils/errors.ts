/**
 * Stable, machine-readable error codes returned in `error.code`.
 *
 * The enum value is the wire string, so renaming a member is a breaking change
 * for clients. Add new codes here rather than inlining string literals.
 * See `api/docs/ERROR_TAXONOMY.md` for the code → HTTP status mapping.
 */
export enum ErrorCode {
  // Request and input problems (4xx)
  VALIDATION_ERROR = 'VALIDATION_ERROR',
  PAYLOAD_TOO_LARGE = 'PAYLOAD_TOO_LARGE',
  UNAUTHORIZED = 'UNAUTHORIZED',
  FORBIDDEN = 'FORBIDDEN',
  NOT_FOUND = 'NOT_FOUND',
  CONFLICT = 'CONFLICT',
  RATE_LIMITED = 'RATE_LIMITED',

  // Protocol and funds
  INSUFFICIENT_BALANCE = 'INSUFFICIENT_BALANCE', // reserved, not yet emitted
  CONTRACT_ERROR = 'CONTRACT_ERROR',

  // Compliance gate (403)
  SANCTIONED_ADDRESS = 'SANCTIONED_ADDRESS',
  OFAC_MATCH = 'OFAC_MATCH',
  KYC_REQUIRED = 'KYC_REQUIRED',
  RESTRICTED_JURISDICTION = 'RESTRICTED_JURISDICTION',
  LIMIT_EXCEEDED = 'LIMIT_EXCEEDED',
  AML_HIGH_RISK = 'AML_HIGH_RISK',
  AML_CRITICAL = 'AML_CRITICAL',

  // Upstream and server problems (5xx)
  NETWORK_ERROR = 'NETWORK_ERROR',
  INTERNAL_SERVER_ERROR = 'INTERNAL_SERVER_ERROR',
}

/**
 * Standard error response format
 */
export interface ErrorResponse {
  success: false;
  error: {
    code: string;
    message: string;
    details?: unknown;
  };
  requestId?: string;
}

export class ApiError extends Error {
  constructor(
    public statusCode: number,
    public message: string,
    public code: ErrorCode,
    public details?: unknown,
    public isOperational = true
  ) {
    super(message);
    Object.setPrototypeOf(this, ApiError.prototype);
  }
}

export class ValidationError extends ApiError {
  constructor(message: string) {
    super(400, message, ErrorCode.VALIDATION_ERROR);
    Object.setPrototypeOf(this, ValidationError.prototype);
  }
}

export class UnauthorizedError extends ApiError {
  constructor(message = 'Unauthorized') {
    super(401, message, ErrorCode.UNAUTHORIZED);
    Object.setPrototypeOf(this, UnauthorizedError.prototype);
  }
}

export class ForbiddenError extends ApiError {
  constructor(message = 'Forbidden') {
    super(403, message, ErrorCode.FORBIDDEN);
    Object.setPrototypeOf(this, ForbiddenError.prototype);
  }
}

export class NotFoundError extends ApiError {
  constructor(message = 'Resource not found') {
    super(404, message, ErrorCode.NOT_FOUND);
    Object.setPrototypeOf(this, NotFoundError.prototype);
  }
}

export class ConflictError extends ApiError {
  constructor(message: string) {
    super(409, message, ErrorCode.CONFLICT);
    Object.setPrototypeOf(this, ConflictError.prototype);
  }
}

export class PayloadTooLargeError extends ApiError {
  constructor(message = 'Request body too large') {
    super(413, message, ErrorCode.PAYLOAD_TOO_LARGE);
    Object.setPrototypeOf(this, PayloadTooLargeError.prototype);
  }
}

export class InternalServerError extends ApiError {
  constructor(message = 'Internal server error') {
    super(500, message, ErrorCode.INTERNAL_SERVER_ERROR);
    Object.setPrototypeOf(this, InternalServerError.prototype);
  }
}
