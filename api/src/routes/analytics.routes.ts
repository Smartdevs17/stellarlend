import { Router } from 'express';
import * as analyticsController from '../controllers/analytics.controller';

const router: Router = Router();

/**
 * @openapi
 * /analytics/historical-rates:
 *   get:
 *     summary: Historical APY rates
 *     description: Returns deposit and borrow APY over a configurable time range.
 *     tags:
 *       - Analytics
 *     parameters:
 *       - in: query
 *         name: timeRange
 *         schema:
 *           type: string
 *           enum: [1d, 7d, 30d, 1y]
 *           default: 7d
 *       - in: query
 *         name: poolAddress
 *         schema:
 *           type: string
 */
router.get('/historical-rates', analyticsController.historicalRates);

/**
 * @openapi
 * /analytics/pool-utilization:
 *   get:
 *     summary: Pool utilization over time
 *     tags:
 *       - Analytics
 *     parameters:
 *       - in: query
 *         name: timeRange
 *         schema:
 *           type: string
 *           enum: [1d, 7d, 30d, 1y]
 *           default: 7d
 *       - in: query
 *         name: poolAddress
 *         schema:
 *           type: string
 */
router.get('/pool-utilization', analyticsController.poolUtilization);

/**
 * @openapi
 * /analytics/rate-comparison:
 *   get:
 *     summary: Rate comparison across pools
 *     tags:
 *       - Analytics
 */
router.get('/rate-comparison', analyticsController.rateComparison);

/**
 * @openapi
 * /analytics/revenue:
 *   get:
 *     summary: Protocol revenue tracking
 *     tags:
 *       - Analytics
 *     parameters:
 *       - in: query
 *         name: timeRange
 *         schema:
 *           type: string
 *           enum: [1d, 7d, 30d, 1y]
 *           default: 30d
 */
router.get('/revenue', analyticsController.protocolRevenue);

/**
 * @openapi
 * /analytics/summary:
 *   get:
 *     summary: Analytics summary snapshot
 *     tags:
 *       - Analytics
 */
router.get('/summary', analyticsController.analyticsSummary);

/**
 * @openapi
 * /analytics/export:
 *   get:
 *     summary: Export analytics data
 *     tags:
 *       - Analytics
 *     parameters:
 *       - in: query
 *         name: format
 *         schema:
 *           type: string
 *           enum: [json, csv]
 *           default: json
 *       - in: query
 *         name: timeRange
 *         schema:
 *           type: string
 *           enum: [1d, 7d, 30d, 1y]
 *           default: 7d
 */
router.get('/export', analyticsController.analyticsExport);

export default router;
