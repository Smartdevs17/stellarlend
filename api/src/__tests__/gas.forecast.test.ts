/**
 * Gas Forecasting Model Tests (issue #717)
 *
 * Validates the Holt-Winters / regression forecast model, the walk-forward
 * backtester, and the `GasEstimatorService.forecastGas` integration.
 */

import {
  forecastSeries,
  linearRegression,
  detectSeasonality,
  seasonLengthForPeriod,
} from '../services/gas/model/forecastModel';
import { backtest } from '../services/gas/model/backtester';
import { gasEstimatorService } from '../services/gas/estimator';
import { TimeSeriesPoint } from '../services/gas/model/types';

/**
 * Generate a deterministic seasonal + trend series that the model must learn:
 * `value = level + trend*i + season*sin(2*pi*i/seasonLength)` with small noise.
 */
function makeSeasonalSeries(
  count: number,
  seasonLength: number,
  opts: { level?: number; trend?: number; season?: number; noise?: number } = {}
): number[] {
  const { level = 10000, trend = 60, season = 800, noise = 0.02 } = opts;
  let seed = 42;
  const rnd = () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 4294967296;
  };
  const values: number[] = [];
  for (let i = 0; i < count; i++) {
    const cycle = season * Math.sin((2 * Math.PI * i) / seasonLength);
    const n = (rnd() - 0.5) * 2 * noise * level;
    values.push(Math.round(level + trend * i + cycle + n));
  }
  return values;
}

describe('forecastModel', () => {
  describe('linearRegression', () => {
    it('returns horizon many points and follows an upward trend', () => {
      const series = makeSeasonalSeries(30, 7, { season: 0, noise: 0, trend: 100 });
      const values = linearRegression(series, 5);
      expect(values).toHaveLength(5);
      expect(values[0]).toBeGreaterThan(series[0]);
      expect(values[4]).toBeGreaterThan(values[0]);
    });
  });

  describe('detectSeasonality', () => {
    it('detects a seasonal pattern when present', () => {
      const series = makeSeasonalSeries(28, 7, { noise: 0.02 });
      expect(detectSeasonality(series, 7)).toBe(true);
    });

    it('does not detect seasonality for a flat/pure-trend series', () => {
      const series = makeSeasonalSeries(28, 7, { season: 0, noise: 0.01 });
      expect(detectSeasonality(series, 7)).toBe(false);
    });
  });

  describe('forecastSeries', () => {
    it('selects holt-winters for a seasonal series and forecasts ahead', () => {
      const series = makeSeasonalSeries(56, 7, { noise: 0.02 });
      const fit = forecastSeries(series, 6, 7);
      expect(fit.model).toBe('holt-winters');
      expect(fit.seasonalityDetected).toBe(true);
      expect(fit.values).toHaveLength(6);
      for (const v of fit.values) {
        expect(v).toBeGreaterThan(0);
      }
    });

    it('falls back to linear regression for a non-seasonal series', () => {
      const series = makeSeasonalSeries(20, 7, { season: 0 });
      const fit = forecastSeries(series, 4, 7);
      expect(fit.model).toBe('linear-regression');
      expect(fit.seasonalityDetected).toBe(false);
    });

    it('achieves high accuracy on a seasonal series within a few steps', () => {
      const series = makeSeasonalSeries(50, 7, { noise: 0.01, season: 700 });
      const fit = forecastSeries(series.slice(0, 42), 8, 7);
      let within10 = 0;
      if (fit.values[0] !== undefined) {
        const epsilon = series[42];
        if (epsilon > 0) {
          const firstPct = Math.abs(fit.values[0] - epsilon) / epsilon;
          if (firstPct <= 0.1) within10 += 1;
        }
      }
      expect(within10).toBe(1);
    });
  });
});

describe('backtest', () => {
  it('achieves >=90% within-10% accuracy on a 7d-seasonal series', () => {
    const values = makeSeasonalSeries(84, 7, { noise: 0.005, season: 700, trend: 40 });
    const now = Date.now();
    const points: TimeSeriesPoint[] = values.map((v, i) => ({
      timestamp: now - (values.length - 1 - i) * 3600000,
      value: v,
    }));

    const result = backtest(points, { operation: 'liquidation', period: '7d' });
    expect(result.sampleCount).toBeGreaterThan(0);
    expect(result.within10Percent).toBeGreaterThanOrEqual(90);
    expect(result.mape).toBeLessThanOrEqual(12);
  });

  it('reports zero samples when the series is too short', () => {
    const points: TimeSeriesPoint[] = [
      { timestamp: 1, value: 100 },
      { timestamp: 2, value: 110 },
      { timestamp: 3, value: 105 },
    ];
    const result = backtest(points, { operation: 'borrow', period: '7d' });
    expect(result.sampleCount).toBe(0);
    expect(result.within10Percent).toBe(0);
  });
});

describe('seasonLengthForPeriod', () => {
  it('maps periods to cycle lengths', () => {
    expect(seasonLengthForPeriod('24h')).toBe(24);
    expect(seasonLengthForPeriod('7d')).toBe(7);
    expect(seasonLengthForPeriod('30d')).toBe(7);
  });
});

describe('gasEstimatorService.forecastGas', () => {
  it('returns a well-formed forecast response', async () => {
    const forecast = await gasEstimatorService.forecastGas('liquidation', 6, '7d');
    expect(forecast.operation).toBe('liquidation');
    expect(forecast.horizon).toBe(6);
    expect(forecast.period).toBe('7d');
    expect(forecast.points).toHaveLength(6);
    expect(forecast.backtest).toBeDefined();
    expect(forecast.confidence).toBe('high');
    for (const point of forecast.points) {
      expect(Number(point.forecast)).toBeGreaterThan(0);
      expect(Number(point.upper)).toBeGreaterThanOrEqual(Number(point.lower));
    }
  });

  it('clamps the horizon to a sane upper bound', async () => {
    const forecast = await gasEstimatorService.forecastGas('deposit', 500, '24h');
    expect(forecast.horizon).toBe(24);
    expect(forecast.points).toHaveLength(24);
  });
});
