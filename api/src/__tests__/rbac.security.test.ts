import jwt from 'jsonwebtoken';
import { Request, Response, NextFunction } from 'express';
import {
  requireRole,
  requirePermission,
  resolveRole,
  resolveActor,
  assignRole,
  getRbacAuditContext,
  getCurrentRoleAssignments,
} from '../middleware/rbac';
import { config } from '../config';
import { UnauthorizedError, ValidationError } from '../utils/errors';

describe('RBAC Security & Anti-Spoofing (Issue #1004)', () => {
  const secret = config.auth.jwtSecret || 'test-secret';
  const adminAddress = 'GADMINADDRESS1234567890123456789012345678901234567890';
  const operatorAddress = 'GOPERATORADDRESS12345678901234567890123456789012345678';
  const userAddress = 'GUSERADDRESS123456789012345678901234567890123456789012';
  const attackerAddress = 'GATTACKER12345678901234567890123456789012345678901234';

  beforeAll(() => {
    // Assign operator role to operatorAddress via legitimate admin call
    assignRole('admin', operatorAddress, 'operator');
    assignRole('admin', adminAddress, 'admin');
  });

  function mockRequest(options: {
    headers?: Record<string, string>;
    user?: { address: string };
  }): Request {
    return {
      headers: options.headers || {},
      user: options.user,
      method: 'POST',
      path: '/api/protocol/pause',
      id: 'req-test-1',
    } as unknown as Request;
  }

  const mockResponse = {} as Response;
  const mockNext: NextFunction = jest.fn();

  beforeEach(() => {
    jest.clearAllMocks();
  });

  describe('Unauthenticated Spoofing Resistance', () => {
    it('rejects unauthenticated request attempting to spoof X-User-Role: admin', () => {
      const req = mockRequest({
        headers: {
          'x-user-role': 'admin',
          'x-user-address': attackerAddress,
        },
      });

      expect(() => {
        requireRole('admin')(req, mockResponse, mockNext);
      }).toThrow(UnauthorizedError);
      expect(mockNext).not.toHaveBeenCalled();
    });

    it('rejects unauthenticated request attempting to spoof X-User-Role: operator', () => {
      const req = mockRequest({
        headers: {
          'x-user-role': 'operator',
          'x-user-address': attackerAddress,
        },
      });

      expect(() => {
        requireRole('operator')(req, mockResponse, mockNext);
      }).toThrow(UnauthorizedError);
      expect(mockNext).not.toHaveBeenCalled();
    });

    it('defaults unauthenticated requests without spoofed headers to viewer role', () => {
      const req = mockRequest({
        headers: {},
      });

      expect(resolveRole(req)).toBe('viewer');
      // Should pass viewer checks
      expect(() => {
        requireRole('viewer')(req, mockResponse, mockNext);
      }).not.toThrow();
      expect(mockNext).toHaveBeenCalled();
    });
  });

  describe('Authenticated Role Enforcement', () => {
    it('allows verified admin with valid token to pass admin requirement', () => {
      const token = jwt.sign({ address: adminAddress }, secret);
      const req = mockRequest({
        headers: {
          authorization: `Bearer ${token}`,
        },
      });

      expect(() => {
        requireRole('admin')(req, mockResponse, mockNext);
      }).not.toThrow();
      expect(mockNext).toHaveBeenCalled();
    });

    it('allows verified operator to pass operator requirement but blocks from admin', () => {
      const token = jwt.sign({ address: operatorAddress }, secret);
      const req = mockRequest({
        headers: {
          authorization: `Bearer ${token}`,
        },
      });

      expect(() => {
        requireRole('operator')(req, mockResponse, mockNext);
      }).not.toThrow();

      expect(() => {
        requireRole('admin')(req, mockResponse, mockNext);
      }).toThrow(UnauthorizedError);
    });

    it('blocks normal user even if they send X-User-Role: admin header', () => {
      const token = jwt.sign({ address: userAddress }, secret);
      const req = mockRequest({
        headers: {
          authorization: `Bearer ${token}`,
          'x-user-role': 'admin',
        },
      });

      expect(() => {
        requireRole('admin')(req, mockResponse, mockNext);
      }).toThrow(UnauthorizedError);
      expect(mockNext).not.toHaveBeenCalled();
    });

    it('rejects request when X-User-Address does not match authenticated token identity', () => {
      const token = jwt.sign({ address: userAddress }, secret);
      const req = mockRequest({
        headers: {
          authorization: `Bearer ${token}`,
          'x-user-address': adminAddress, // Impersonation attempt
        },
      });

      expect(() => {
        resolveActor(req, false);
      }).toThrow(UnauthorizedError);
    });
  });

  describe('Audit Context Integrity', () => {
    it('binds audit context to authentic user identity rather than spoofed headers', () => {
      const token = jwt.sign({ address: adminAddress }, secret);
      const req = mockRequest({
        headers: {
          authorization: `Bearer ${token}`,
          'x-user-role': 'user', // header claims lower role
        },
      });

      const audit = getRbacAuditContext(req);
      expect(audit.actor).toBe(adminAddress);
      expect(audit.role).toBe('admin');
    });
  });
});
