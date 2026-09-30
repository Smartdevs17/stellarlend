import { Request, Response, NextFunction } from 'express';
import { config } from '../config';
import { redisCacheService, HotCacheKeyKind } from '../services/redisCache.service';
import { readCacheService, ttlSeconds } from '../services/readCache.service';

/**
 * Response cache for read-heavy GET routes.
 *
 * On a miss the JSON body of a 2xx response is stored under the request path
 * plus its sorted query string; later identical requests are answered from
 * the cache without running the handler. Every response carries an `X-Cache`
 * header (HIT, MISS, BYPASS or SKIP). Clients bypass the cache with
 * `Cache-Control: no-cache`, and writes invalidate the `http` kind.
 */

export interface ReadCacheMiddlewareOptions {
  /** Cache kind used for keys and metrics (default `http`). */
  kind?: HotCacheKeyKind;
  /** Entry TTL in milliseconds (default READ_CACHE_DEFAULT_TTL_MS). */
  ttlMs?: number;
  /** Override the cache identity derived from the request. */
  keyBuilder?: (req: Request) => string;
}

export interface CachedHttpResponse {
  status: number;
  body: unknown;
  cacheControl?: string;
}

function serializeQueryValue(value: unknown): string {
  if (typeof value === 'string') {
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item) => serializeQueryValue(item)).join(',');
  }
  return JSON.stringify(value);
}

/** Path plus query string with keys sorted, so parameter order does not split the cache. */
export function buildReadCacheKey(req: Request): string {
  const query = Object.keys(req.query)
    .sort()
    .map(
      (name) =>
        `${encodeURIComponent(name)}=${encodeURIComponent(serializeQueryValue(req.query[name]))}`
    )
    .join('&');
  const path = `${req.baseUrl}${req.path}`;
  return query ? `${path}?${query}` : path;
}

export function cacheReadResponse(options: ReadCacheMiddlewareOptions = {}) {
  const kind: HotCacheKeyKind = options.kind ?? 'http';

  return (req: Request, res: Response, next: NextFunction): void => {
    if (req.method !== 'GET' || !readCacheService.isEnabled()) {
      next();
      return;
    }

    const id = options.keyBuilder ? options.keyBuilder(req) : buildReadCacheKey(req);
    if (id.length > config.readCache.maxQueryLength) {
      res.setHeader('X-Cache', 'SKIP');
      next();
      return;
    }

    const requestCacheControl = String(req.headers['cache-control'] ?? '');
    const bypass = /\bno-(cache|store)\b/i.test(requestCacheControl);
    const key = readCacheService.buildKey(kind, id);
    const ttl = ttlSeconds(options.ttlMs ?? config.readCache.defaultTtlMs);

    const serve = async (): Promise<void> => {
      if (bypass) {
        res.setHeader('X-Cache', 'BYPASS');
      } else {
        const hit = await redisCacheService.get<CachedHttpResponse>(key);
        if (hit && typeof hit.status === 'number') {
          readCacheService.recordHit(kind);
          res.setHeader('X-Cache', 'HIT');
          if (hit.cacheControl) {
            res.setHeader('Cache-Control', hit.cacheControl);
          }
          res.status(hit.status).json(hit.body);
          return;
        }
        readCacheService.recordMiss(kind);
        res.setHeader('X-Cache', 'MISS');
      }

      const originalJson = res.json.bind(res);
      res.json = ((body: unknown) => {
        if (res.statusCode >= 200 && res.statusCode < 300) {
          const cacheControl = res.getHeader('Cache-Control');
          const entry: CachedHttpResponse = {
            status: res.statusCode,
            body,
            ...(typeof cacheControl === 'string' ? { cacheControl } : {}),
          };
          readCacheService.recordStore(kind);
          void redisCacheService.set(key, entry, ttl);
        }
        return originalJson(body);
      }) as Response['json'];

      next();
    };

    serve().catch(next);
  };
}
