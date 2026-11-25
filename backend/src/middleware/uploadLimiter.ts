import { Request, Response, NextFunction } from 'express';

export interface UploadLimitsOptions {
  maxBytesDefault: number;
  routeLimits?: Record<string, number>;
}

export const DEFAULT_MAX_UPLOAD_BYTES = 5 * 1024 * 1024; // 5 MB

export function createContentLengthLimiter(options: Partial<UploadLimitsOptions> = {}) {
  const defaultLimit = options.maxBytesDefault || DEFAULT_MAX_UPLOAD_BYTES;
  const routeLimits = options.routeLimits || {};

  return (req: Request, res: Response, next: NextFunction): void => {
    const contentLengthHeader = req.headers['content-length'];

    const targetLimit = routeLimits[req.path] || defaultLimit;

    if (!contentLengthHeader) {
      if (['POST', 'PUT', 'PATCH'].includes(req.method) && req.headers['transfer-encoding'] !== 'chunked') {
        res.status(411).json({ error: 'Length Required: Content-Length header is missing' });
        return;
      }
      return next();
    }

    const length = parseInt(contentLengthHeader, 10);
    if (isNaN(length)) {
      res.status(400).json({ error: 'Bad Request: Invalid Content-Length header' });
      return;
    }

    if (length > targetLimit) {
      res.status(413).json({
        error: 'Payload Too Large',
        message: `Content-Length of ${length} bytes exceeds the maximum allowable ceiling of ${targetLimit} bytes`,
      });
      return;
    }

    // Stream-level byte counter guard against spoofed content-length
    let receivedBytes = 0;
    const onData = (chunk: Buffer) => {
      receivedBytes += chunk.length;
      if (receivedBytes > targetLimit) {
        req.removeListener('data', onData);
        req.destroy(new Error('Payload size exceeded limit during streaming'));
        if (!res.headersSent) {
          res.status(413).json({ error: 'Payload Too Large: Stream exceeded allowed limit' });
        }
      }
    };

    req.on('data', onData);
    next();
  };
}
