/**
 * Protocol Health Score routes — Issue #692
 *
 * Mounted under `/api/health` alongside `health.routes.ts`, which owns the
 * `/live` and `/ready` probes. Everything here hangs off `/score`, so the two
 * cannot collide.
 */

import { Router } from 'express';
import { healthScoreController } from '../controllers/health.controller';

const router: Router = Router();

/**
 * @openapi
 * /health/score:
 *   get:
 *     summary: Current composite protocol health score
 *     description: >
 *       A 0-100 score built from six weighted components: capital efficiency,
 *       liquidity, bad debt, concentration, oracle health and governance health.
 *     tags:
 *       - Protocol Health
 *     responses:
 *       200:
 *         description: The current score and its components
 *         content:
 *           application/json:
 *             schema:
 *               type: object
 *               properties:
 *                 success:
 *                   type: boolean
 *                 data:
 *                   type: object
 *                   properties:
 *                     overallScore:
 *                       type: number
 *                       example: 78.4
 *                     components:
 *                       type: object
 *                     weights:
 *                       type: object
 *                     timestamp:
 *                       type: string
 *                       format: date-time
 *       500:
 *         description: Score computation failed
 */
router.get('/score', (req, res) => healthScoreController.getScore(req, res));

/**
 * @openapi
 * /health/score/summary:
 *   get:
 *     summary: Score, trend and alerts in one response
 *     description: >
 *       Built for dashboards: a single request instead of three that could
 *       return mutually inconsistent snapshots.
 *     tags:
 *       - Protocol Health
 *     parameters:
 *       - in: query
 *         name: window
 *         schema:
 *           type: integer
 *           minimum: 1
 *           maximum: 365
 *           default: 30
 *         description: History points to include in the trend
 *     responses:
 *       200:
 *         description: Combined score, trend and alerts
 *       400:
 *         description: Invalid window
 */
router.get('/score/summary', (req, res) => healthScoreController.getSummary(req, res));

/**
 * @openapi
 * /health/score/history:
 *   get:
 *     summary: Recorded health score history
 *     tags:
 *       - Protocol Health
 *     parameters:
 *       - in: query
 *         name: limit
 *         schema:
 *           type: integer
 *           minimum: 1
 *           maximum: 365
 *         description: Most recent N points
 *     responses:
 *       200:
 *         description: History points, oldest first
 *       400:
 *         description: Invalid limit
 */
router.get('/score/history', (req, res) => healthScoreController.getHistory(req, res));

/**
 * @openapi
 * /health/score/trend:
 *   get:
 *     summary: Direction and rate of change of the health score
 *     description: >
 *       Least-squares slope, volatility, per-component movers and a one-step
 *       projection, computed from the recorded history.
 *     tags:
 *       - Protocol Health
 *     parameters:
 *       - in: query
 *         name: window
 *         schema:
 *           type: integer
 *           minimum: 1
 *           maximum: 365
 *           default: 30
 *     responses:
 *       200:
 *         description: Trend analysis
 *         content:
 *           application/json:
 *             schema:
 *               type: object
 *               properties:
 *                 data:
 *                   type: object
 *                   properties:
 *                     direction:
 *                       type: string
 *                       enum: [improving, stable, declining]
 *                     change:
 *                       type: number
 *                     slope:
 *                       type: number
 *                     volatility:
 *                       type: number
 *                     movers:
 *                       type: array
 *                       items:
 *                         type: object
 *       400:
 *         description: Invalid window
 */
router.get('/score/trend', (req, res) => healthScoreController.getTrend(req, res));

/**
 * @openapi
 * /health/score/alerts:
 *   get:
 *     summary: Health score threshold breaches
 *     description: Returns an empty array when nothing has breached.
 *     tags:
 *       - Protocol Health
 *     responses:
 *       200:
 *         description: Active alerts and the configured threshold
 */
router.get('/score/alerts', (req, res) => healthScoreController.getAlerts(req, res));

/**
 * @openapi
 * /health/score/weights:
 *   get:
 *     summary: Component weights used by the composite score
 *     tags:
 *       - Protocol Health
 *     responses:
 *       200:
 *         description: Current weights
 */
router.get('/score/weights', (req, res) => healthScoreController.getWeights(req, res));

/**
 * @openapi
 * /health/score/weights:
 *   put:
 *     summary: Update component weights (governance-controlled)
 *     tags:
 *       - Protocol Health
 *     requestBody:
 *       required: true
 *       content:
 *         application/json:
 *           schema:
 *             type: object
 *     responses:
 *       200:
 *         description: Updated weights
 *       400:
 *         description: Invalid weights
 */
router.put('/score/weights', (req, res) => healthScoreController.updateWeights(req, res));

/**
 * @openapi
 * /health/score/alert-threshold:
 *   put:
 *     summary: Update the alert threshold (governance-controlled)
 *     tags:
 *       - Protocol Health
 *     requestBody:
 *       required: true
 *       content:
 *         application/json:
 *           schema:
 *             type: object
 *             properties:
 *               threshold:
 *                 type: number
 *     responses:
 *       200:
 *         description: Updated threshold
 *       400:
 *         description: Invalid threshold
 */
router.put('/score/alert-threshold', (req, res) =>
  healthScoreController.updateAlertThreshold(req, res),
);

export default router;
