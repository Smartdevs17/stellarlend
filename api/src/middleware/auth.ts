import { Request, Response, NextFunction } from 'express';
import jwt from 'jsonwebtoken';
import { config } from '../config';
import { UnauthorizedError, ForbiddenError } from '../utils/errors';
import { apiKeyService, type ApiKeyRecord, type ApiScope } from '../services/apiKey.service';

export interface AuthRequest extends Request {
  user?: {
    address: string;
  };
  /** Issue #1374 – authenticated API key metadata (never the raw key). */
  apiKey?: {
    id: string;
    scopes: ApiScope[];
  };
}

export const authenticateToken = (req: AuthRequest, res: Response, next: NextFunction) => {
  const authHeader = req.headers['authorization'];
  const token = authHeader && authHeader.split(' ')[1];

  if (!token) {
    throw new UnauthorizedError('Access token required');
  }

  try {
    const decoded = jwt.verify(token, config.auth.jwtSecret) as { address: string };
    req.user = decoded;
    next();
  } catch (error) {
    throw new UnauthorizedError('Invalid or expired token');
  }
};

export const generateToken = (address: string): string => {
  return jwt.sign({ address }, config.auth.jwtSecret, {
    expiresIn: config.auth.jwtExpiresIn,
  } as jwt.SignOptions);
};

/**
 * Issue #387 – API key authentication middleware.
 * Issue #1374 – Attaches the key id + scopes so routes can enforce them.
 * Issue #1375 – Records one usage event per authenticated request.
 *
 * Accepts a raw API key in the X-API-Key header.
 * The key is verified against the bcrypt hash stored in apiKeyService.
 * The raw key is never logged.
 */
export const authenticateApiKey = async (
  req: AuthRequest,
  _res: Response,
  next: NextFunction
): Promise<void> => {
  const rawKey = req.headers['x-api-key'];

  if (!rawKey || typeof rawKey !== 'string') {
    throw new UnauthorizedError('X-API-Key header is required');
  }

  const result = await apiKeyService.verify(rawKey);
  if (!result.valid || !result.record) {
    throw new UnauthorizedError('Invalid or revoked API key');
  }

  const record = result.record;
  req.apiKey = { id: record.id, scopes: record.scopes ?? ['*'] };
  apiKeyService.recordUsage(record.id, req.baseUrl + req.path);

  // Attach a synthetic user identity from the key record
  req.user = { address: record.createdBy ?? 'api-key-user' };
  next();
};

/**
 * Issue #1374 – Scope enforcement middleware.
 *
 * Must run after `authenticateApiKey`. Accepts one or more scopes and passes
 * when the key holds any of them (or the wildcard `*`).
 */
export const requireScope = (...required: ApiScope[]) => {
  return (req: AuthRequest, _res: Response, next: NextFunction): void => {
    if (!req.apiKey) {
      throw new UnauthorizedError('API key authentication required');
    }
    const granted = req.apiKey.scopes;
    const allowed =
      granted.includes('*') || required.some((scope) => granted.includes(scope));
    if (!allowed) {
      throw new ForbiddenError(
        `API key is missing required scope: ${required.join(' or ')}`
      );
    }
    next();
  };
};

export function hasScope(
  record: Pick<ApiKeyRecord, 'scopes'> | undefined,
  required: ApiScope
): boolean {
  return apiKeyService.hasScope(record, required);
}

/**
 * Require authentication via either Bearer JWT token or X-API-Key header.
 */
export const authenticateAdmin = async (
  req: AuthRequest,
  res: Response,
  next: NextFunction
): Promise<void> => {
  const authHeader = req.headers['authorization'];
  if (authHeader && authHeader.startsWith('Bearer ')) {
    return authenticateToken(req, res, next);
  }
  const apiKey = req.headers['x-api-key'];
  if (apiKey) {
    return authenticateApiKey(req, res, next);
  }
  throw new UnauthorizedError('Authentication required: provide Bearer token or X-API-Key header');
};
