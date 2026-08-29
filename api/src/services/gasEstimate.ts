/**
 * Gas Estimation Service (#838)
 *
 * Provides a user-facing gas estimation model that predicts transaction costs
 * for all lending protocol operations before the user submits the transaction.
 *
 * Architecture:
 *   - GasEstimateService wraps the core GasEstimatorService (gas/estimator.ts)
 *     and exposes a simplified interface suited for the /gas-estimate API route.
 *   - Estimation logic, baseline constants, and accuracy tracking live in
 *     gas/estimator.ts; this file is the thin integration layer.
 */

export {
  GasEstimatorService,
  gasEstimatorService,
} from './gas/estimator';

export type {
  GasOperation,
  GasCostEstimate,
  GasCostBreakdown,
  GasOptimizationSuggestion,
  HistoricalGasData,
  GasEstimateRequest,
  GasComparisonResponse,
  GasCostAlert,
  GasAlertConfig,
  GasAccuracyReport,
  BatchGasEstimate,
  GasTimingRecommendation,
  GasAnalyticsReport,
} from '../types/gas';
