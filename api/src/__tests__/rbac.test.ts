import { Request, Response, NextFunction } from 'express';
import {
  assignRole,
  getCurrentRoleAssignments,
  requireRole,
  scheduleRevocation,
} from '../middleware/rbac';
import { UnauthorizedError } from '../utils/errors';
import { AuthRequest } from '../middleware/auth';

function mockReq(user?: { address: string; role?: string }): Request {
  return {
    headers: {},
    method: 'POST',
    path: '/test',
    user,
  } as unknown as Request;
}

describe('RBAC hierarchy', () => {
  it('allows admin to assign operator role', () => {
    assignRole('admin', 'GTESTOPERATOR', 'operator');
    expect(getCurrentRoleAssignments()['GTESTOPERATOR']).toBe('operator');
  });

  it('requires cool-off for revocation', () => {
    assignRole('admin', 'GTESTADMIN', 'operator');
    scheduleRevocation('GADMIN', 'admin', 'GTESTADMIN', 'operator', 1_000);
    expect(getCurrentRoleAssignments()['GTESTADMIN']).toBe('operator');
  });
});

describe('requireRole authentication (Issue #1089)', () => {
  const res = {} as Response;
  const next: NextFunction = jest.fn();

  it('allows a JWT-authenticated admin', () => {
    const req = mockReq({ address: 'GADMIN', role: 'admin' });
    expect(() => requireRole('admin')(req, res, next)).not.toThrow();
    expect(next).toHaveBeenCalled();
  });

  it('rejects an insufficient JWT role', () => {
    const req = mockReq({ address: 'GVIEWER', role: 'viewer' });
    expect(() => requireRole('admin')(req, res, next)).toThrow(UnauthorizedError);
  });

  it('rejects forged x-user-role headers without a signed JWT identity', () => {
    const req = {
      headers: { 'x-user-role': 'admin', 'x-user-address': 'GATTACKER' },
      method: 'POST',
      path: '/protocol/pause',
    } as unknown as Request;
    expect((req as AuthRequest).user).toBeUndefined();
    expect(() => requireRole('admin')(req, res, next)).toThrow(UnauthorizedError);
  });

  it('rejects unauthenticated callers', () => {
    expect(() => requireRole('operator')(mockReq(), res, next)).toThrow(UnauthorizedError);
  });
});
