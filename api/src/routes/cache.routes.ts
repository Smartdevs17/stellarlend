import { Router } from 'express';
import * as cacheController from '../controllers/cache.controller';
import { requireRole } from '../middleware/rbac';

const router: Router = Router();

/**
 * @openapi
 * /cache/stats:
 *   get:
 *     summary: Read cache, cache store and prefetch metrics
 *     description: Hit, miss, load and invalidation counters per cache kind, the underlying store counters, and the prefetch scheduler state including the current hot keys.
 *     tags:
 *       - Cache
 *     responses:
 *       200:
 *         description: Cache metrics
 *         content:
 *           application/json:
 *             schema:
 *               type: object
 *               properties:
 *                 timestamp:
 *                   type: string
 *                   format: date-time
 *                 readCache:
 *                   type: object
 *                 store:
 *                   type: object
 *                 prefetch:
 *                   type: object
 */
router.get('/stats', cacheController.getCacheStats);

/**
 * @openapi
 * /cache/invalidate:
 *   post:
 *     summary: Invalidate cached read data
 *     description: Drops every read cache kind when the body is empty, one kind when `kind` is given, or a single entry when both `kind` and `id` are given. Requires the operator role.
 *     tags:
 *       - Cache
 *     requestBody:
 *       required: false
 *       content:
 *         application/json:
 *           schema:
 *             type: object
 *             properties:
 *               kind:
 *                 type: string
 *                 enum: [protocol, pool, position, gas, price, simulation, http]
 *               id:
 *                 type: string
 *                 description: Entry id within the kind (for example a user address for `position`)
 *     responses:
 *       200:
 *         description: Invalidation applied
 *       400:
 *         description: Validation error
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 *       401:
 *         description: Operator role required
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 */
router.post('/invalidate', requireRole('operator'), cacheController.invalidateCache);

/**
 * @openapi
 * /cache/prefetch:
 *   get:
 *     summary: Prefetch scheduler state and hot keys
 *     tags:
 *       - Cache
 *     responses:
 *       200:
 *         description: Prefetch statistics
 */
router.get('/prefetch', cacheController.getPrefetchStats);

/**
 * @openapi
 * /cache/prefetch/run:
 *   post:
 *     summary: Refresh hot cache entries now
 *     description: Runs one prefetch pass immediately instead of waiting for the scheduler. Requires the operator role.
 *     tags:
 *       - Cache
 *     responses:
 *       200:
 *         description: Run summary with refreshed, failed and skipped counts
 *       401:
 *         description: Operator role required
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 */
router.post('/prefetch/run', requireRole('operator'), cacheController.runPrefetch);

export default router;
