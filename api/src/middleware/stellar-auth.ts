import { Request, Response, NextFunction } from 'express';
import { UnauthorizedError } from '../utils/errors';
import logger from '../utils/logger';

export interface AuthenticatedRequest extends Request {
  userAddress?: string;
}

function verifyStellarSignature(userAddress: string, payload: string, signature: string): boolean {
  try {
    const { Keypair, TransactionBuilder } = require('stellar-sdk');
    return Keypair.fromPublicKey(userAddress).verify(Buffer.from(payload), Buffer.from(signature, 'base64'));
  } catch (err) {
    logger.warn('Stellar signature verification failed', { userAddress, error: String(err) });
    return false;
  }
}

export function requireStellarAuth(req: AuthenticatedRequest, res: Response, next: NextFunction) {
  const userAddress = req.headers['x-user-address'] as string;
  const signature = req.headers['x-stellar-signature'] as string;
  const timestamp = req.headers['x-payload-timestamp'] as string;

  if (!userAddress || !signature || !timestamp) {
    throw new UnauthorizedError('Missing authentication credentials (address, signature, or timestamp)');
  }

  const freshnessWindowMs = 5 * 60 * 1000;
  if (Math.abs(Date.now() - Number(timestamp)) > freshnessWindowMs) {
    throw new UnauthorizedError('Expired request signature');
  }

  const payloadToVerify = `${req.method}:${req.originalUrl}:${JSON.stringify(req.body || {})}:${timestamp}`;

  if (!verifyStellarSignature(userAddress, payloadToVerify, signature)) {
    throw new UnauthorizedError('Invalid cryptographic signature for user address');
  }

  req.userAddress = userAddress;
  next();
}
