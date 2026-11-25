import { describe, it, expect, vi } from 'vitest';
import { createContentLengthLimiter } from '../uploadLimiter';

describe('createContentLengthLimiter', () => {
  it('allows payloads within the ceiling limit', () => {
    const limiter = createContentLengthLimiter({ maxBytesDefault: 1000 });
    const req: any = {
      method: 'POST',
      path: '/api/v1/upload',
      headers: { 'content-length': '500' },
      on: vi.fn(),
      removeListener: vi.fn(),
    };
    const res: any = {
      status: vi.fn().mockReturnThis(),
      json: vi.fn(),
    };
    const next = vi.fn();

    limiter(req, res, next);
    expect(next).toHaveBeenCalled();
    expect(res.status).not.toHaveBeenCalled();
  });

  it('rejects payloads exceeding the ceiling limit with HTTP 413', () => {
    const limiter = createContentLengthLimiter({ maxBytesDefault: 1000 });
    const req: any = {
      method: 'POST',
      path: '/api/v1/upload',
      headers: { 'content-length': '1500' },
      on: vi.fn(),
    };
    const res: any = {
      status: vi.fn().mockReturnThis(),
      json: vi.fn(),
    };
    const next = vi.fn();

    limiter(req, res, next);
    expect(next).not.toHaveBeenCalled();
    expect(res.status).toHaveBeenCalledWith(413);
  });

  it('returns HTTP 411 when Content-Length is missing on POST requests', () => {
    const limiter = createContentLengthLimiter({ maxBytesDefault: 1000 });
    const req: any = {
      method: 'POST',
      path: '/api/v1/upload',
      headers: {},
      on: vi.fn(),
    };
    const res: any = {
      status: vi.fn().mockReturnThis(),
      json: vi.fn(),
    };
    const next = vi.fn();

    limiter(req, res, next);
    expect(res.status).toHaveBeenCalledWith(411);
  });
});
