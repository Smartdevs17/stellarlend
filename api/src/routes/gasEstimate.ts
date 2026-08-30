/**
 * Gas Estimation Route (#838)
 *
 * Dedicated entry point for the gas estimation model that predicts user
 * transaction costs before submission.  Delegates to the existing gas
 * controller / estimator pipeline which is exercised by the main gas
 * routes, so all estimation, caching, accuracy tracking and alert
 * logic lives in one place.
 *
 * Mount point: /api/gas-estimate
 *
 * Endpoints
 * ---------
 * POST /                  – estimate cost for a single operation
 * POST /batch             – estimate cost for multiple operations
 * GET  /timing/:operation – optimal execution timing recommendation
 * GET  /compare           – cost comparison across all operations
 */

import { Router } from 'express';
import { gasController } from '../controllers/gas.controller';

const router = Router();

/**
 * @route   POST /api/gas-estimate
 * @desc    Predict transaction cost for a single lending operation
 * @body    {
 *            operation: GasOperation,
 *            userAddress: string,
 *            assetAddress?: string,
 *            amount: string,
 *            includeOptimizations?: boolean,
 *            includeHistorical?: boolean
 *          }
 * @returns GasCostEstimate with breakdown, suggestions, and confidence score
 * @access  Public
 */
router.post('/', (req, res) => gasController.estimateGas(req, res));

/**
 * @route   POST /api/gas-estimate/batch
 * @desc    Predict transaction costs for multiple operations in one call.
 *          Returns individual estimates plus batched-execution savings.
 * @body    { operations: GasEstimateRequest[] }
 * @returns BatchGasEstimate
 * @access  Public
 */
router.post('/batch', (req, res) => gasController.estimateBatchCost(req, res));

/**
 * @route   GET /api/gas-estimate/timing/:operation
 * @desc    Return the optimal time window to execute an operation for
 *          lowest expected gas cost.
 * @param   operation – one of deposit | withdraw | borrow | repay |
 *                      liquidation | flash_loan
 * @returns GasTimingRecommendation
 * @access  Public
 */
router.get('/timing/:operation', (req, res) =>
  gasController.getTimingRecommendation(req, res)
);

/**
 * @route   GET /api/gas-estimate/compare
 * @desc    Compare estimated gas costs across all supported operations,
 *          ranked cheapest-to-most-expensive.
 * @returns GasComparisonResponse
 * @access  Public
 */
router.get('/compare', (req, res) => gasController.compareOperations(req, res));

export default router;
