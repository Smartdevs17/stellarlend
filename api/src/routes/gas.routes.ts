/**
 * Gas Cost Estimation Routes
 * 
 * API endpoints for gas cost estimation and optimization
 */

import { Router } from 'express';
import { gasController } from '../controllers/gas.controller';
import { estimateTransactionFees } from '../controllers/transactionSimulation.controller';
import { cacheReadResponse } from '../middleware/readCache.middleware';

const router = Router();

/**
 * @route   POST /api/gas/estimate
 * @desc    Estimate gas cost for a specific operation
 * @body    { operation, userAddress, assetAddress?, amount, includeOptimizations?, includeHistorical? }
 * @access  Public
 */
router.post('/estimate', (req, res) => gasController.estimateGas(req, res));

/**
 * @openapi
 * /lending/gas/estimate-transaction:
 *   post:
 *     summary: Estimate the fees of a user's transaction
 *     description: Simulates the given transaction envelope (or a lending operation built for the user) once and returns the inclusion fee, resource fee, total and a recommended fee with a safety margin, plus the resources the invocation will consume.
 *     tags:
 *       - Gas
 *     requestBody:
 *       required: true
 *       content:
 *         application/json:
 *           schema:
 *             type: object
 *             properties:
 *               transactionXdr:
 *                 type: string
 *                 description: Base64 transaction envelope. When present the operation fields are ignored.
 *               operation:
 *                 type: string
 *                 enum: [deposit, borrow, repay, withdraw]
 *               userAddress:
 *                 type: string
 *                 description: Stellar public key (Ed25519)
 *               amount:
 *                 type: string
 *                 description: Amount as a positive integer string (stroops)
 *               assetAddress:
 *                 type: string
 *                 description: Optional asset contract address
 *               feeMarginPercent:
 *                 type: integer
 *                 minimum: 0
 *                 maximum: 100
 *                 default: 10
 *                 description: Safety margin added to the total fee
 *     responses:
 *       200:
 *         description: Fee estimate derived from simulation
 *       400:
 *         description: Validation error
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 *       422:
 *         description: The simulation failed, so no fee can be estimated
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 *       502:
 *         description: Soroban RPC was unreachable
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 */
router.post('/estimate-transaction', estimateTransactionFees);

/**
 * @route   GET /api/gas/historical/:operation
 * @desc    Get historical gas data for an operation
 * @query   period (optional): '24h', '7d', '30d' (default: '30d')
 * @access  Public
 */
router.get('/historical/:operation', cacheReadResponse(), (req, res) => gasController.getHistoricalData(req, res));

/**
 * @route   GET /api/gas/chart/:operation
 * @desc    Get historical gas chart data for visualization
 * @query   period (optional): '24h', '7d', '30d' (default: '7d')
 * @access  Public
 */
router.get('/chart/:operation', cacheReadResponse(), (req, res) => gasController.getChartData(req, res));

/**
 * @route   GET /api/gas/compare
 * @desc    Compare gas costs across all operations
 * @access  Public
 */
router.get('/compare', cacheReadResponse(), (req, res) => gasController.compareOperations(req, res));

/**
 * @route   POST /api/gas/alerts
 * @desc    Configure gas cost alert
 * @body    { userAddress?, operation, threshold, enabled }
 * @access  Public
 */
router.post('/alerts', (req, res) => gasController.configureAlert(req, res));

/**
 * @route   GET /api/gas/alerts
 * @desc    Get all alerts for a user
 * @query   userAddress (optional)
 * @access  Public
 */
router.get('/alerts', (req, res) => gasController.getAlerts(req, res));

/**
 * @route   POST /api/gas/accuracy
 * @desc    Record actual gas cost for accuracy tracking
 * @body    { operation, estimatedCost, actualCost, txHash }
 * @access  Public
 */
router.post('/accuracy', (req, res) => gasController.recordActualCost(req, res));

/**
 * @route   GET /api/gas/accuracy
 * @desc    Get accuracy report
 * @query   period (optional): '24h', '7d', '30d' (default: '7d')
 * @access  Public
 */
router.get('/accuracy', (req, res) => gasController.getAccuracyReport(req, res));

/**
 * @route   POST /api/gas/batch-estimate
 * @desc    Estimate gas cost for batch operations
 * @body    { operations: GasEstimateRequest[] }
 * @access  Public
 */
router.post('/batch-estimate', (req, res) => gasController.estimateBatchCost(req, res));

/**
 * @route   GET /api/gas/timing/:operation
 * @desc    Get timing recommendation for optimal execution
 * @access  Public
 */
router.get('/timing/:operation', (req, res) => gasController.getTimingRecommendation(req, res));

/**
 * @route   GET /api/gas/analytics
 * @desc    Get gas usage analytics and optimization metrics
 * @query   period (optional): '24h', '7d', '30d'
 * @access  Public
 */
router.get('/analytics', cacheReadResponse(), (req, res) => gasController.getAnalytics(req, res));

/**
 * @route   GET /api/gas/forecast/:operation
 * @desc    Forecast future gas cost for an operation (time-series model, #717)
 * @query   horizon (optional, default 6), period (optional): '24h','7d','30d'
 * @access  Public
 */
router.get('/forecast/:operation', cacheReadResponse(), (req, res) => gasController.forecastGas(req, res));

export default router;
