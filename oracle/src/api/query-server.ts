import http from 'http';
import { logger } from '../utils/logger.js';
import type { AggregatedPrice } from '../types/index.js';
import type { PriceHistoryService } from '../services/price-history.js';
import type { OracleMode } from '../types/index.js';
import { buildOpenApiDocument } from './openapi.js';

/**
 * Issue #1371 / #1372 / #1373 – Read-side query API.
 *
 * Provides pull-model reads (`/prices`), historical price queries with
 * filters + cursor pagination (`/prices/:asset/history`), and the generated
 * OpenAPI document (`/openapi.json`).
 */

export interface QueryServerOptions {
  port?: number;
  mode?: OracleMode;
  priceHistory: PriceHistoryService;
  /** Pull on demand from the oracle (push mode may serve its cached values). */
  getPrices: (assets?: string[]) => Promise<AggregatedPrice[]>;
}

export const DEFAULT_QUERY_PORT = 3002;

function sendJson(res: http.ServerResponse, status: number, body: unknown): void {
  const payload = JSON.stringify(body);
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Content-Length': Buffer.byteLength(payload),
    'Access-Control-Allow-Origin': '*',
  });
  res.end(payload);
}

function serializePrice(price: AggregatedPrice) {
  return {
    asset: price.asset,
    price: price.price.toString(),
    timestamp: price.timestamp,
    confidence: price.confidence,
    sources: price.sources.map((source) => source.source),
  };
}

function parseIntParam(value: string | null, name: string): number | undefined {
  if (value === null || value === '') return undefined;
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || !Number.isInteger(parsed)) {
    throw new Error(`${name} must be an integer`);
  }
  return parsed;
}

export class QueryServer {
  private server?: http.Server;
  private options: QueryServerOptions;

  constructor(options: QueryServerOptions) {
    this.options = options;
  }

  start(): Promise<number> {
    const port = this.options.port ?? DEFAULT_QUERY_PORT;
    this.server = http.createServer((req, res) => {
      this.handle(req, res).catch((error) => {
        logger.error('Query server request failed', { error: String(error) });
        sendJson(res, 500, { success: false, error: 'Internal server error' });
      });
    });

    return new Promise((resolve) => {
      this.server!.listen(port, () => {
        const address = this.server!.address();
        const boundPort = typeof address === 'object' && address ? address.port : port;
        logger.info('Oracle query server listening', {
          port: boundPort,
          mode: this.options.mode ?? 'push',
        });
        resolve(boundPort);
      });
      this.server!.on('error', (err: NodeJS.ErrnoException) => {
        if (err.code === 'EADDRINUSE') {
          logger.warn(`Query server could not bind to port ${port} (already in use)`);
        } else {
          logger.error('Query server error', { error: err.message });
        }
        resolve(port);
      });
    });
  }

  stop(): Promise<void> {
    return new Promise((resolve) => {
      if (!this.server) return resolve();
      this.server.close(() => {
        this.server = undefined;
        resolve();
      });
    });
  }

  private async handle(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
    const url = new URL(req.url ?? '/', 'http://localhost');
    const path = url.pathname;

    if (req.method !== 'GET') {
      sendJson(res, 405, { success: false, error: 'Method not allowed' });
      return;
    }

    if (path === '/health') {
      sendJson(res, 200, { status: 'ok', service: 'stellarlend-oracle-query' });
      return;
    }

    if (path === '/openapi.json') {
      sendJson(res, 200, buildOpenApiDocument());
      return;
    }

    if (path === '/prices') {
      const assetFilter = (url.searchParams.get('asset') ?? '')
        .split(',')
        .map((asset) => asset.trim().toUpperCase())
        .filter(Boolean);
      const prices = await this.options.getPrices(assetFilter.length > 0 ? assetFilter : undefined);
      sendJson(res, 200, {
        success: true,
        mode: this.options.mode ?? 'push',
        data: prices.map(serializePrice),
        timestamp: new Date().toISOString(),
      });
      return;
    }

    const assetMatch = path.match(/^\/prices\/([^/]+)$/);
    if (assetMatch) {
      const asset = decodeURIComponent(assetMatch[1]!).toUpperCase();
      const prices = await this.options.getPrices([asset]);
      const price = prices.find((entry) => entry.asset.toUpperCase() === asset);
      if (!price) {
        sendJson(res, 404, { success: false, error: `No price available for ${asset}` });
        return;
      }
      sendJson(res, 200, {
        success: true,
        mode: this.options.mode ?? 'push',
        data: serializePrice(price),
      });
      return;
    }

    const historyMatch = path.match(/^\/prices\/([^/]+)\/history$/);
    if (historyMatch) {
      const asset = decodeURIComponent(historyMatch[1]!).toUpperCase();
      try {
        const from = parseIntParam(url.searchParams.get('from'), 'from');
        const to = parseIntParam(url.searchParams.get('to'), 'to');
        const limit = parseIntParam(url.searchParams.get('limit'), 'limit');
        const cursor = parseIntParam(url.searchParams.get('cursor'), 'cursor');

        const page = this.options.priceHistory.queryHistory(asset, {
          from,
          to,
          limit,
          cursor,
        });

        sendJson(res, 200, {
          success: true,
          data: {
            ...page,
            entries: page.entries.map((entry) => ({
              price: entry.price.toString(),
              timestamp: entry.timestamp,
            })),
          },
          timestamp: new Date().toISOString(),
        });
      } catch (error: any) {
        sendJson(res, 400, { success: false, error: error?.message ?? 'Invalid query' });
      }
      return;
    }

    sendJson(res, 404, { success: false, error: 'Not found' });
  }
}

export function createQueryServer(options: QueryServerOptions): QueryServer {
  return new QueryServer(options);
}
