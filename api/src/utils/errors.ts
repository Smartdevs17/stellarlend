export enum ErrorCode {
  VALIDATION_ERROR = 'VALIDATION_ERROR',
  INSUFFICIENT_BALANCE = 'INSUFFICIENT_BALANCE',
  CONTRACT_ERROR = 'CONTRACT_ERROR',
  NETWORK_ERROR = 'NETWORK_ERROR',
  RATE_LIMITED = 'RATE_LIMITED',
  UNAUTHORIZED = 'UNAUTHORIZED',
  FORBIDDEN = 'FORBIDDEN',
  NOT_FOUND = 'NOT_FOUND',
  CONFLICT = 'CONFLICT',
  EXPIRED = 'EXPIRED',
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

export class ExpiredError extends ApiError {
  constructor(message = 'Resource has expired') {
    super(410, message, ErrorCode.EXPIRED);
    Object.setPrototypeOf(this, ExpiredError.prototype);
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
    super(413, message, ErrorCode.VALIDATION_ERROR);
    Object.setPrototypeOf(this, PayloadTooLargeError.prototype);
  }
}

export class InternalServerError extends ApiError {
  constructor(message = 'Internal server error') {
    super(500, message, ErrorCode.INTERNAL_SERVER_ERROR);
    Object.setPrototypeOf(this, InternalServerError.prototype);
  }
}
