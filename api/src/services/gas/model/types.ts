/**
 * Gas Forecasting Types
 *
 * Types describing time-series forecasts and backtest results produced by the
 * gas forecasting model (issue #717). Forecasts predict future gas cost for a
 * given operation from its observed historical cost series.
 */

import { GasOperation } from '../../../types/gas';

export type { GasOperation };

/** Seasonality period length (number of samples per repeated cycle). */
export const FORECAST_SEASON_24H = 24;
export const FORECAST_SEASON_7D = 7;

export interface GasForecastRequest {
  operation: GasOperation;
  /** Number of periods ahead to forecast (>= 1). */
  horizon: number;
  /** Granularity / length of the input historical series. */
  period: '24h' | '7d' | '30d';
}

export interface GasForecastPoint {
  /** ISO timestamp for the forecast bucket. */
  timestamp: string;
  /** Forecast cost in stroops. */
  forecast: string;
  /** 80% prediction-interval lower bound in stroops. */
  lower: string;
  /** 80% prediction-interval upper bound in stroops. */
  upper: string;
}

export type ForecastModelName = 'holt-winters' | 'linear-regression';

export interface GasForecast {
  operation: GasOperation;
  points: GasForecastPoint[];
  model: ForecastModelName;
  /** True when a seasonal component was detected and used. */
  seasonalityDetected: boolean;
  confidence: 'high' | 'medium' | 'low';
  timestamp: string;
}

export interface GasBacktestResult {
  operation: GasOperation;
  /** Mean absolute percentage error over the backtest windows. */
  mape: number;
  /** Percentage of out-of-sample forecasts within 10% of actual. */
  within10Percent: number;
  /** Number of out-of-sample validation points evaluated. */
  sampleCount: number;
  /** Name of the best-performing model. */
  model: ForecastModelName;
}

export interface TimeSeriesPoint {
  /** Unix epoch milliseconds. */
  timestamp: number;
  /** Observed cost in stroops. */
  value: number;
}
