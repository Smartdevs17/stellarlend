import { Request, Response, NextFunction } from 'express';
import { redisCacheService } from '../services/redisCache.service';
import { readCacheService, isReadCacheKind } from '../services/readCache.service';
import { prefetchService } from '../services/prefetch.service';
import { ValidationError } from '../utils/errors';

/** GET /api/cache/stats */
export const getCacheStats = (_req: Request, res: Response, next: NextFunction) => {
  try {
    res.status(200).json({
      timestamp: new Date().toISOString(),
      readCache: readCacheService.getStats(),
      store: redisCacheService.getMetrics(),
      prefetch: prefetchService.getStats(),
    });
  } catch (error) {
    next(error);
  }
};

/** POST /api/cache/invalidate  body: { kind?: string, id?: string } */
export const invalidateCache = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { kind, id } = (req.body ?? {}) as { kind?: unknown; id?: unknown };

    if (kind === undefined) {
      if (id !== undefined) {
        throw new ValidationError('kind is required when id is provided');
      }
      await readCacheService.invalidateAll();
      res.status(200).json({ success: true, scope: 'all' });
      return;
    }

    if (!isReadCacheKind(kind)) {
      throw new ValidationError('kind must be a read cache kind');
    }
    if (id !== undefined && (typeof id !== 'string' || !id.trim())) {
      throw new ValidationError('id must be a non-empty string');
    }

    await readCacheService.invalidate(kind, typeof id === 'string' ? id.trim() : undefined);
    res.status(200).json({
      success: true,
      scope: typeof id === 'string' ? 'entry' : 'kind',
      kind,
      ...(typeof id === 'string' ? { id: id.trim() } : {}),
    });
  } catch (error) {
    next(error);
  }
};

/** GET /api/cache/prefetch */
export const getPrefetchStats = (_req: Request, res: Response, next: NextFunction) => {
  try {
    res.status(200).json(prefetchService.getStats());
  } catch (error) {
    next(error);
  }
};

/** POST /api/cache/prefetch/run */
export const runPrefetch = async (_req: Request, res: Response, next: NextFunction) => {
  try {
    const result = await prefetchService.runOnce();
    res.status(200).json({ success: true, ...result });
  } catch (error) {
    next(error);
  }
};
