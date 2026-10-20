/**
 * Assets Domain Routes (v1)
 *
 * Read-only asset metadata under /v1/assets:
 * - GET /v1/assets          list every supported asset
 * - GET /v1/assets/:symbol  metadata for one asset (case-insensitive symbol)
 *
 * Responses are cached with the shared read cache (see `cacheReadResponse`)
 * and advertise the same lifetime to clients via `Cache-Control`.
 */

import { Router, Request, Response, NextFunction } from 'express';
import { config } from '../../../config';
import { cacheReadResponse } from '../../../middleware/readCache.middleware';
import { assetMetadataService } from '../../../services/assetMetadata.service';
import { ApiError, ErrorCode } from '../../../utils/errors';
import { assetShutdownService } from '../../../services/assetShutdown.service';

const router = Router();

function setCacheControl(res: Response): void {
  res.setHeader(
    'Cache-Control',
    `public, max-age=${Math.floor(config.cache.assetMetadataTtlMs / 1000)}`
  );
}

/**
 * @openapi
 * /assets:
 *   get:
 *     summary: List supported assets
 *     description: Returns metadata for every asset supported by the protocol. Cached.
 *     tags:
 *       - Assets
 *     responses:
 *       200:
 *         description: List of asset metadata
 *         content:
 *           application/json:
 *             schema:
 *               type: object
 *               properties:
 *                 success: { type: boolean, example: true }
 *                 data:
 *                   type: array
 *                   items:
 *                     $ref: '#/components/schemas/AssetMetadata'
 */
router.get(
  '/',
  cacheReadResponse({ ttlMs: config.cache.assetMetadataTtlMs }),
  (_req: Request, res: Response, next: NextFunction) => {
    try {
      setCacheControl(res);
      res.status(200).json({ success: true, data: assetMetadataService.listAssets() });
    } catch (error) {
      next(error);
    }
  }
);

/**
 * @openapi
 * /assets/{symbol}:
 *   get:
 *     summary: Get metadata for one asset
 *     description: Looks up an asset by symbol (case-insensitive). Cached.
 *     tags:
 *       - Assets
 *     parameters:
 *       - in: path
 *         name: symbol
 *         required: true
 *         schema: { type: string, example: XLM }
 *     responses:
 *       200:
 *         description: Asset metadata
 *         content:
 *           application/json:
 *             schema:
 *               type: object
 *               properties:
 *                 success: { type: boolean, example: true }
 *                 data:
 *                   $ref: '#/components/schemas/AssetMetadata'
 *       404:
 *         description: Unknown asset symbol
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 *
 * components:
 *   schemas:
 *     AssetMetadata:
 *       type: object
 *       properties:
 *         symbol: { type: string, example: XLM }
 *         name: { type: string, example: Stellar Lumens }
 *         category: { type: string, enum: [native, stablecoin, crypto, defi] }
 *         decimals: { type: integer, example: 7 }
 *         isNative: { type: boolean }
 */
router.get(
  '/:symbol',
  cacheReadResponse({ ttlMs: config.cache.assetMetadataTtlMs }),
  (req: Request, res: Response, next: NextFunction) => {
    try {
      const asset = assetMetadataService.getAsset(req.params.symbol ?? '');
      if (!asset) {
        throw new ApiError(404, `Asset '${req.params.symbol}' not found`, ErrorCode.NOT_FOUND);
      }
      setCacheControl(res);
      res.status(200).json({ success: true, data: asset });
    } catch (error) {
      next(error);
    }
  }
);

/**
 * @openapi
 * /assets/{symbol}/shutdown:
 *   post:
 *     summary: Emergency shutdown of one asset
 *     description: >
 *       Scoped emergency shutdown for a single asset. While shut down, the asset
 *       is flagged as disabled in metadata responses. Reversible via the same
 *       endpoint with `{ "shutdown": false }`. Idempotent.
 *     tags:
 *       - Assets
 *     parameters:
 *       - in: path
 *         name: symbol
 *         required: true
 *         schema: { type: string, example: XLM }
 *     requestBody:
 *       required: false
 *       content:
 *         application/json:
 *           schema:
 *             type: object
 *             properties:
 *               shutdown: { type: boolean, default: true }
 *               reason: { type: string }
 *     responses:
 *       200:
 *         description: Shutdown state applied
 *       404:
 *         description: Unknown asset symbol
 */
router.post(
  '/:symbol/shutdown',
  (req: Request, res: Response, next: NextFunction) => {
    try {
      const symbol = req.params.symbol ?? '';
      const asset = assetMetadataService.getAsset(symbol);
      if (!asset) {
        throw new ApiError(404, `Asset '${symbol}' not found`, ErrorCode.NOT_FOUND);
      }
      const body = (req.body ?? {}) as { shutdown?: unknown; reason?: unknown };
      const shutdown = body.shutdown === undefined ? true : Boolean(body.shutdown);
      const reason = typeof body.reason === 'string' ? body.reason : undefined;
      const state = assetShutdownService.setShutdown(asset.symbol, shutdown, reason);
      res.status(200).json({ success: true, data: state });
    } catch (error) {
      next(error);
    }
  }
);

/**
 * @openapi
 * /assets/{symbol}/shutdown:
 *   get:
 *     summary: Get shutdown state for one asset
 *     tags:
 *       - Assets
 *     parameters:
 *       - in: path
 *         name: symbol
 *         required: true
 *         schema: { type: string, example: XLM }
 *     responses:
 *       200:
 *         description: Current shutdown state
 *       404:
 *         description: Unknown asset symbol
 */
router.get(
  '/:symbol/shutdown',
  (req: Request, res: Response, next: NextFunction) => {
    try {
      const symbol = req.params.symbol ?? '';
      const asset = assetMetadataService.getAsset(symbol);
      if (!asset) {
        throw new ApiError(404, `Asset '${symbol}' not found`, ErrorCode.NOT_FOUND);
      }
      res.status(200).json({ success: true, data: assetShutdownService.getState(asset.symbol) });
    } catch (error) {
      next(error);
    }
  }
);

export default router;
