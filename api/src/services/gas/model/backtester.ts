/**
 * Gas Forecast Backtester (issue #717)
 *
 * Validates the forecasting model using an expanding-window walk-forward
 * split: for each validation step we train on the preceding window and measure
 * error against the held-out point. Produces a `GasBacktestResult` with MAPE
 * and the fraction of forecasts within 10% of the actual value.
 */

import { forecastSeries, seasonLengthForPeriod } from './forecastModel';
import { GasBacktestResult, GasOperation, TimeSeriesPoint } from './types';
import { GasOperation as GasOp } from '../../../types/gas';

// Require enough history for the seasonality to be identifiable before scoring
// a forecast (2 weekly cycles is the smallest meaningful window).
const MIN_TRAIN = 14;
const DEFAULT_HORIZON = 1;

export interface BacktestOptions {
  operation: GasOperation;
  period: '24h' | '7d' | '30d';
  horizon?: number;
}

/**
 * Unwrap the cash-operation union into values accepted by the estimator.
 * Kept internal to the backtester so the estimator/caller decides the window.
 */
function asGasOp(op: GasOperation): GasOp {
  return op;
}

/**
 * Run a walk-forward backtest over a fixed-cadence time series.
 * `series` must be evenly spaced (one sample per period).
 */
export function backtest(
  points: TimeSeriesPoint[],
  options: BacktestOptions
): GasBacktestResult {
  const { operation, period, horizon = DEFAULT_HORIZON } = options;
  const seasonLength = seasonLengthForPeriod(period);
  const values = points.map((p) => p.value);

  const errors: number[] = [];
  let within10 = 0;

  // Expanding window: grow the training window one step at a time.
  for (let trainEnd = MIN_TRAIN; trainEnd < values.length; trainEnd++) {
    const train = values.slice(0, trainEnd);
    const actual = values[trainEnd] ?? 0;
    const stepHorizon = Math.min(horizon, values.length - trainEnd);

    const fit = forecastSeries(train, stepHorizon, seasonLength);
    const forecast = fit.values[0] ?? 0;

    if (actual === 0) {
      continue;
    }
    const pctError = Math.abs(forecast - actual) / actual;
    errors.push(pctError * 100);
    if (pctError <= 0.1) {
      within10 += 1;
    }
  }

  const sampleCount = errors.length;
  const mape =
    sampleCount > 0
      ? errors.reduce((s, v) => s + v, 0) / sampleCount
      : Number.POSITIVE_INFINITY;
  const within10Percent =
    sampleCount > 0 ? (within10 / sampleCount) * 100 : 0;

  return {
    operation: asGasOp(operation),
    mape: Math.round(mape * 100) / 100,
    within10Percent: Math.round(within10Percent * 100) / 100,
    sampleCount,
    model: 'holt-winters',
  };
}
