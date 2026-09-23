/**
 * Gas Forecasting Model (issue #717)
 *
 * Provides a real, deterministic time-series forecasting model for gas costs:
 *  - Holt-Winters (triple exponential smoothing) with multiplicative
 *    seasonality for operations that exhibit a daily/weekly cycle.
 *  - Linear-regression fallback for non-seasonal series.
 *
 * The model is pure (no I/O) and testable. Callers provide an array of
 * historical `{ timestamp, value }` points sampled at a fixed cadence.
 */

import {
  ForecastModelName,
  FORECAST_SEASON_24H,
  FORECAST_SEASON_7D,
} from './types';

export interface FitResult {
  values: number[]; // forecast for the next `horizon` periods
  model: ForecastModelName;
  seasonalityDetected: boolean;
}

const DEFAULT_ALPHA = 0.35;
const DEFAULT_BETA = 0.2;
const DEFAULT_GAMMA = 0.35;

/** Minimal number of full seasonal cycles required to fit seasonality. */
const MIN_CYCLES = 2;

/**
 * Detect whether a series has a meaningful repeating pattern by comparing
 * within-cycle variance to total variance after removing any linear drift.
 */
export function detectSeasonality(
  series: number[],
  seasonLength: number
): boolean {
  if (series.length < seasonLength * MIN_CYCLES) {
    return false;
  }

  const n = series.length;
  // Detrend with a best-fit line so a steady rise/fall does not mask (or fake)
  // within-cycle structure.
  const meanX = (n - 1) / 2;
  const rawMean = series.reduce((s, v) => s + v, 0) / n;
  let num = 0;
  let den = 0;
  for (let i = 0; i < n; i++) {
    const v = series[i] ?? 0;
    num += (i - meanX) * (v - rawMean);
    den += (i - meanX) * (i - meanX);
  }
  const slope = den === 0 ? 0 : num / den;
  const intercept = rawMean - slope * meanX;
  const detrended = series.map(
    (v, i) => (v ?? 0) - (intercept + slope * i)
  );

  const mean = detrended.reduce((sum, v) => sum + v, 0) / Math.max(n, 1);
  const totalVar = detrended.reduce(
    (sum, v) => sum + (v - mean) * (v - mean),
    0
  );
  if (totalVar === 0) {
    return false;
  }

  // Residual variance after removing the per-season-mean pattern.
  const seasons = Math.floor(n / seasonLength);
  const seasonSums = new Array<number>(seasonLength).fill(0);
  const seasonCounts = new Array<number>(seasonLength).fill(0);
  detrended.forEach((v, i) => {
    const phase = i % seasonLength;
    seasonSums[phase] = (seasonSums[phase] ?? 0) + v;
    seasonCounts[phase] = (seasonCounts[phase] ?? 0) + 1;
  });
  const seasonMeans = seasonSums.map(
    (s, idx) => s / Math.max(seasonCounts[idx] ?? 0, 1)
  );

  let residualVar = 0;
  detrended.forEach((v, i) => {
    const seasonMean = seasonMeans[i % seasonLength] ?? 0;
    const d = v - seasonMean;
    residualVar += d * d;
  });

  // If the repeating within-cycle pattern explains most variance, seasonal.
  const unexplained = residualVar / Math.max(totalVar, 1e-9);
  return unexplained < 0.7 && seasons >= MIN_CYCLES;
}

/**
 * Holt-Winters multiplicative triple exponential smoothing.
 */
export function holtWinters(
  series: number[],
  horizon: number,
  seasonLength: number
): FitResult {
  const n = series.length;
  if (n < 2) {
    const fallback = linearRegression(series, horizon);
    return {
      values: fallback,
      model: 'linear-regression',
      seasonalityDetected: false,
    };
  }

  const isSeasonal = detectSeasonality(series, seasonLength);
  if (!isSeasonal) {
    const fallback = linearRegression(series, horizon);
    return {
      values: fallback,
      model: 'linear-regression',
      seasonalityDetected: false,
    };
  }

  const L = seasonLength;
  // Initial level: mean of the first full season.
  const firstSeason = series.slice(0, L);
  const level0 =
    firstSeason.reduce((sum, v) => sum + v, 0) / Math.max(firstSeason.length, 1);
  if (level0 === 0) {
    const fallback = linearRegression(series, horizon);
    return {
      values: fallback,
      model: 'linear-regression',
      seasonalityDetected: false,
    };
  }

  // Initial trend: slope between the first and last season means.
  const seasonCount = Math.floor(n / L);
  const lastSeasonStart = (seasonCount - 1) * L;
  const lastSeason = series.slice(lastSeasonStart, lastSeasonStart + L);
  const lastMean =
    lastSeason.reduce((s, v) => s + v, 0) / Math.max(lastSeason.length, 1);
  const trend0 = (lastMean - level0) / Math.max(seasonCount - 1, 1);

  // Initial multiplicative seasonal indices.
  const seasonMeans = new Array<number>(seasonCount).fill(0);
  for (let i = 0; i < n; i++) {
    const bucket = Math.floor(i / L);
    seasonMeans[bucket] = (seasonMeans[bucket] ?? 0) + (series[i] ?? 0);
  }
  seasonMeans.forEach((s, i) => {
    seasonMeans[i] = (s ?? 0) / L;
  });

  const seasonal0 = new Array<number>(L).fill(1);
  for (let i = 0; i < n; i++) {
    const mean = seasonMeans[Math.floor(i / L)] || level0;
    seasonal0[i % L] = (seasonal0[i % L] ?? 0) + (series[i] ?? 0) / mean;
  }
  // Normalize seasonal indices to sum to L (multiplicative convention).
  const seasonSum = seasonal0.reduce((s, v) => s + v, 0);
  const normalize = seasonSum > 0 ? L / seasonSum : 1;
  for (let i = 0; i < L; i++) {
    seasonal0[i] = (seasonal0[i] ?? 1) * normalize;
  }

  const alpha = DEFAULT_ALPHA;
  const beta = DEFAULT_BETA;
  const gamma = DEFAULT_GAMMA;

  let level = level0;
  let trend = trend0;
  const seasonal = seasonal0.slice();

  // Smooth over the full series.
  for (let i = 0; i < n; i++) {
    const observed = series[i] ?? 0;
    const prevSeasonal = seasonal[i % L] || 1;
    const prevLevel = level;

    const newLevel =
      alpha * (observed / prevSeasonal) + (1 - alpha) * (level + trend);
    const newTrend =
      beta * (newLevel - level) + (1 - beta) * trend;
    const newSeasonal =
      gamma * (observed / newLevel) + (1 - gamma) * prevSeasonal;

    level = newLevel;
    trend = newTrend;
    seasonal[i % L] = newSeasonal;
  }

  // Generate out-of-sample forecasts.
  const values: number[] = [];
  for (let m = 1; m <= horizon; m++) {
    const seasonIndex = (n + m - 1) % L;
    const seasonalFactor = seasonal[seasonIndex] || 1;
    const base = level + m * trend;
    values.push(Math.max(0, Math.round(base * seasonalFactor)));
  }

  return {
    values,
    model: 'holt-winters',
    seasonalityDetected: true,
  };
}

/**
 * Simple linear regression (least squares) on the series index, used as a
 * non-seasonal fallback.
 */
export function linearRegression(
  series: number[],
  horizon: number
): number[] {
  const n = series.length;
  if (n === 0) {
    return new Array<number>(horizon).fill(0);
  }
  if (n === 1) {
    return new Array<number>(horizon).fill(series[0] ?? 0);
  }

  const meanX = (n - 1) / 2;
  const meanY = series.reduce((s, v) => s + v, 0) / n;

  let num = 0;
  let den = 0;
  for (let i = 0; i < n; i++) {
    num += (i - meanX) * ((series[i] ?? 0) - meanY);
    den += (i - meanX) * (i - meanX);
  }
  const slope = den === 0 ? 0 : num / den;
  const intercept = meanY - slope * meanX;

  const values: number[] = [];
  for (let m = 1; m <= horizon; m++) {
    values.push(Math.max(0, Math.round(intercept + slope * (n - 1 + m))));
  }
  return values;
}

/** 80% prediction-interval width as a fraction of the point forecast. */
export function predictionBand(model: ForecastModelName): number {
  // Holt-Winters usually tightens confidence; regression is wider.
  return model === 'holt-winters' ? 0.12 : 0.2;
}

/**
 * Fit the best available model to `series` for the given season length and
 * horizon, returning both the point forecasts and a label describing the
 * model actually used.
 */
export function forecastSeries(
  series: number[],
  horizon: number,
  seasonLength: number
): FitResult {
  return holtWinters(series, horizon, seasonLength);
}

/** Map a forecast granularity to its seasonal cycle length. */
export function seasonLengthForPeriod(period: string): number {
  if (period === '24h') {
    return FORECAST_SEASON_24H;
  }
  if (period === '7d') {
    return FORECAST_SEASON_7D;
  }
  // 30d defaults to a weekly (7) seasonality, the most common operational cycle.
  return FORECAST_SEASON_7D;
}
