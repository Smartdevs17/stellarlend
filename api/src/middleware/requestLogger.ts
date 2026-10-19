import { Request, Response, NextFunction } from 'express';
import logger from '../utils/logger';

export const SERVICE_NAME = 'stellarlend-api';

const SENSITIVE_FIELDS = new Set([
  'password',
  'secret',
  'token',
  'apiKey',
  'api_key',
  'authorization',
  'x-api-key',
  'privateKey',
  'private_key',
  'mnemonic',
  'seed',
]);

function redactSensitiveHeaders(headers: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(headers)) {
    result[key] = SENSITIVE_FIELDS.has(key.toLowerCase()) ? '[REDACTED]' : value;
  }
  return result;
}

/**
 * Return the W3 trace context for the current request, if one has been
 * assigned by the tracing middleware. This is attached to every structured
 * log line so logs can be correlated with traces.
 */
function traceContext(res: Response): Record<string, string> | undefined {
  const ctx = res.locals?.traceContext as Record<string, string> | undefined;
  return ctx;
}

export function requestLogger(req: Request, res: Response, next: NextFunction): void {
  const startAt = process.hrtime.bigint();

  res.on('finish', () => {
    const durationMs = Number(process.hrtime.bigint() - startAt) / 1_000_000;
    const level = res.statusCode >= 500 ? 'error' : res.statusCode >= 400 ? 'warn' : 'info';

    logger[level]('HTTP request', {
      service: SERVICE_NAME,
      requestId: req.id,
      method: req.method,
      path: req.path,
      statusCode: res.statusCode,
      durationMs: parseFloat(durationMs.toFixed(2)),
      userAgent: req.headers['user-agent'],
      ip: req.ip,
      headers: redactSensitiveHeaders(req.headers as Record<string, unknown>),
      trace: traceContext(res),
    });
  });

  next();
}
