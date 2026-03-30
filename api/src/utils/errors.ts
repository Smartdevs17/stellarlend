export enum ErrorCode {
  VALIDATION_ERROR = 'VALIDATION_ERROR',
  INSUFFICIENT_BALANCE = 'INSUFFICIENT_BALANCE',
  CONTRACT_ERROR = 'CONTRACT_ERROR',
  NETWORK_ERROR = 'NETWORK_ERROR',
  RATE_LIMITED = 'RATE_LIMITED',
  UNAUTHORIZED = 'UNAUTHORIZED',
  NOT_FOUND = 'NOT_FOUND',
  CONFLICT = 'CONFLICT',
  INTERNAL_SERVER_ERROR = 'INTERNAL_SERVER_ERROR',
}

export class ApiError extends Error {
  constructor(
    public statusCode: number,
    public message: string,
    public code: ErrorCode,
    public isOperational = true
  ) {
    super(message);
    Object.setPrototypeOf(this, ApiError.prototype);
  }
}

export class ValidationError extends ApiError {
  constructor(message: string) {
    super(400, message, ErrorCode.VALIDATION_ERROR);
  }
}

export class UnauthorizedError extends ApiError {
  constructor(message = 'Unauthorized') {
    super(401, message, ErrorCode.UNAUTHORIZED);
  }
}

export class NotFoundError extends ApiError {
  constructor(message = 'Resource not found') {
    super(404, message, ErrorCode.NOT_FOUND);
  }
}

export class ConflictError extends ApiError {
  constructor(message: string) {
    super(409, message, ErrorCode.CONFLICT);
  }
}

export class PayloadTooLargeError extends ApiError {
  constructor(message = 'Request body too large') {
    super(413, message);
  }
}

export class InternalServerError extends ApiError {
  constructor(message = 'Internal server error') {
    super(500, message, ErrorCode.INTERNAL_SERVER_ERROR);
  }
}
