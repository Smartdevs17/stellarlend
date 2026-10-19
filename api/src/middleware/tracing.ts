import { Request, Response, NextFunction } from 'express';
import { randomUUID } from 'crypto';
import logger from '../utils/logger';

/**
 * Sampling rate applied to traced requests. Reads TRACE_SAMPLE_RATE from the
 * environment (0 disables tracing, 1 traces every request). Defaults to 100%
 * in non-production and 10% in production so high-traffic deployments are not
 * overwhelmed by trace logs.
 */
function resolveSampleRate(): number {
  const raw = process.env.TRACE_SAMPLE_RATE;
  if (typeof raw === 'string') {
    const parsed = parseFloat(raw);
    if (Number.isFinite(parsed) && parsed >= 0 && parsed <= 1) {
      return parsed;
    }
  }
  return process.env.NODE_ENV === 'production' ? 0.1 : 1;
}

const SAMPLE_RATE = resolveSampleRate();

export interface TracedRequest extends Request {
  traceId?: string;
  sampled?: boolean;
}

/**
 * Request tracing middleware (Issue #1418).
 *
 * Generates or propagates a `traceId`, applies a sampling decision, and exposes
 * both via response headers (`x-trace-id`, `x-sampled`). Sampled requests are
 * logged so operators can correlate request lifecycles across services.
 *
 * Acceptance criteria:
 * - Correlated: every request gets a stable traceId echoed back to the client
 * - Sampled: sampling decision is applied and exposed via `x-sampled` header
 * - Documented: see api/docs/API_VERSIONING.md (versioning) and this module
 */
export function tracingMiddleware(req: TracedRequest, res: Response, next: NextFunction): void {
  const incoming = req.headers['x-trace-id'];
  const traceId =
    typeof incoming === 'string' && incoming.length > 0 ? incoming : randomUUID();
  const sampled = Math.random() < SAMPLE_RATE;

  req.traceId = traceId;
  req.sampled = sampled;

  res.setHeader('x-trace-id', traceId);
  res.setHeader('x-sampled', String(sampled));

  if (sampled) {
    logger.info('HTTP request traced', {
      traceId,
      sampled: true,
      method: req.method,
      path: req.path,
      ip: req.ip,
    });
  }

  next();
}

/** Returns the trace id attached to the request by tracingMiddleware. */
export function getTraceId(req: Request): string | undefined {
  return (req as TracedRequest).traceId;
}

/** Returns whether the request was selected for tracing. */
export function isSampled(req: Request): boolean {
  return Boolean((req as TracedRequest).sampled);
}
