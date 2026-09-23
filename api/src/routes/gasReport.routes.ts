/**
 * Contract gas optimization report — Issue #684
 *
 * Mounted at /api/analytics/gas/contract. Serves the report generated from
 * `stellar-lend/benchmarks` (measured Soroban CPU instructions), not runtime
 * samples — see /api/analytics/gas for those.
 */

import { Router } from 'express';
import { gasReportController } from '../controllers/gasReport.controller';

const router = Router();

/**
 * @openapi
 * /analytics/gas/contract:
 *   get:
 *     summary: Full contract gas report — per-function costs, budgets, regressions, recommendations, trends
 *     tags:
 *       - Gas Report
 *     parameters:
 *       - { in: query, name: format, schema: { type: string, enum: [json, markdown] } }
 *       - { in: query, name: refresh, schema: { type: boolean }, description: 'Bypass the 60s cache' }
 */
router.get('/', (req, res, next) => gasReportController.getReport(req, res, next));

/**
 * @openapi
 * /analytics/gas/contract/budgets:
 *   get:
 *     summary: Budget utilization per function and per operation type
 *     tags:
 *       - Gas Report
 */
router.get('/budgets', (req, res, next) => gasReportController.getBudgets(req, res, next));

/**
 * @openapi
 * /analytics/gas/contract/budgets/{type}:
 *   get:
 *     summary: Budget and functions for one operation type (read, admin, user_write, liquidation, flash_loan, batch)
 *     tags:
 *       - Gas Report
 */
router.get('/budgets/:type', (req, res, next) =>
  gasReportController.getBudgetForType(req, res, next)
);

/**
 * @openapi
 * /analytics/gas/contract/regressions:
 *   get:
 *     summary: Functions whose cost moved against the committed baseline
 *     tags:
 *       - Gas Report
 */
router.get('/regressions', (req, res, next) => gasReportController.getRegressions(req, res, next));

/**
 * @openapi
 * /analytics/gas/contract/recommendations:
 *   get:
 *     summary: Rule-based optimization recommendations, most severe first
 *     tags:
 *       - Gas Report
 *     parameters:
 *       - { in: query, name: severity, schema: { type: string, enum: [critical, high, medium, low] } }
 */
router.get('/recommendations', (req, res, next) =>
  gasReportController.getRecommendations(req, res, next)
);

/**
 * @openapi
 * /analytics/gas/contract/trends:
 *   get:
 *     summary: Historical benchmark trend and per-journey gas totals
 *     tags:
 *       - Gas Report
 */
router.get('/trends', (req, res, next) => gasReportController.getTrends(req, res, next));

export default router;
