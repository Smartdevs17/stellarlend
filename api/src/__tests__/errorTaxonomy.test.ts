import { Request, Response, NextFunction } from 'express';
import { errorHandler } from '../middleware/errorHandler';
import {
  ConflictError,
  ErrorCode,
  InternalServerError,
  NotFoundError,
  PayloadTooLargeError,
  UnauthorizedError,
  ValidationError,
} from '../utils/errors';

describe('error taxonomy', () => {
  it('keeps every wire value equal to its member name', () => {
    // Clients match on these strings, so a silent rename must fail here.
    for (const [name, value] of Object.entries(ErrorCode)) {
      expect(value).toBe(name);
    }
  });

  it.each([
    ['ValidationError', new ValidationError('x'), 400, ErrorCode.VALIDATION_ERROR],
    ['UnauthorizedError', new UnauthorizedError(), 401, ErrorCode.UNAUTHORIZED],
    ['NotFoundError', new NotFoundError(), 404, ErrorCode.NOT_FOUND],
    ['ConflictError', new ConflictError('x'), 409, ErrorCode.CONFLICT],
    ['PayloadTooLargeError', new PayloadTooLargeError(), 413, ErrorCode.PAYLOAD_TOO_LARGE],
    ['InternalServerError', new InternalServerError(), 500, ErrorCode.INTERNAL_SERVER_ERROR],
  ])('%s maps to status %i and code %s', (_name, error, status, code) => {
    expect(error.statusCode).toBe(status);
    expect(error.code).toBe(code);
  });

  it('serialises a 413 through the error handler with PAYLOAD_TOO_LARGE', () => {
    const req = { path: '/api/test', method: 'POST', headers: {} } as unknown as Request;
    const res = {
      status: jest.fn().mockReturnThis(),
      json: jest.fn().mockReturnThis(),
    } as unknown as Response;
    const next = jest.fn() as NextFunction;

    errorHandler(new PayloadTooLargeError('too big'), req, res, next);

    expect(res.status).toHaveBeenCalledWith(413);
    expect(res.json).toHaveBeenCalledWith(
      expect.objectContaining({
        success: false,
        error: expect.objectContaining({ code: 'PAYLOAD_TOO_LARGE', message: 'too big' }),
      })
    );
  });
});
