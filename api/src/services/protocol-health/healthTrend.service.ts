/**
 * Protocol Health Score Trending — Issue #692
 *
 * A single health score says how the protocol is *now*. It does not say whether
 * things are getting better or worse, and a score of 72 falling five points a
 * day is a different situation from a score of 72 that has been flat for a
 * week. This service reads the score history recorded by
 * `healthScore.service` and turns it into a direction, a rate of change, a
 * volatility measure, and the component movers responsible.
 *
 * Everything here is derived from the recorded history — no new data sources —
 * so the trend can never disagree with the scores it is computed from.
 */

import { protocolHealthScoreService } from './healthScore.service';
import {
  HealthScoreComponents,
  HealthScoreHistoryPoint,
} from '../../types/protocolHealth';

/** Which way the score is moving. */
export type TrendDirection = 'improving' | 'stable' | 'declining';

/** A single component's movement over the trend window. */
export interface ComponentMovement {
  component: keyof HealthScoreComponents;
  first: number;
  last: number;
  change: number;
}

export interface HealthScoreTrend {
  /** Number of history points the trend was computed from. */
  sampleSize: number;
  direction: TrendDirection;
  /** Score change across the window (last − first). */
  change: number;
  /** Least-squares slope, in score points per sample. */
  slope: number;
  /** Standard deviation of the scores in the window. */
  volatility: number;
  /** Mean score across the window. */
  average: number;
  first: number;
  last: number;
  /** Components that moved most, largest absolute change first. */
  movers: ComponentMovement[];
  /** Projection one sample ahead from the fitted line, clamped to 0-100. */
  projectedNext: number | null;
  windowStart: string | null;
  windowEnd: string | null;
}

/**
 * Score movement smaller than this over the window counts as flat. Component
 * scores are noisy by a point or two between samples, and calling that a
 * "decline" would train people to ignore the signal.
 */
const STABLE_BAND = 1;

const COMPONENT_KEYS: (keyof HealthScoreComponents)[] = [
  'capitalEfficiency',
  'liquidity',
  'badDebt',
  'concentration',
  'oracleHealth',
  'governanceHealth',
];

function round(value: number): number {
  return Math.round(value * 100) / 100;
}

function clampScore(value: number): number {
  return Math.max(0, Math.min(100, round(value)));
}

/**
 * Least-squares slope of `values` against their index, in units per sample.
 * Returns 0 for fewer than two points, where a slope is undefined.
 */
export function leastSquaresSlope(values: number[]): number {
  const n = values.length;
  if (n < 2) return 0;

  const meanX = (n - 1) / 2;
  const meanY = values.reduce((sum, v) => sum + v, 0) / n;

  let numerator = 0;
  let denominator = 0;
  for (let i = 0; i < n; i += 1) {
    numerator += (i - meanX) * (values[i] - meanY);
    denominator += (i - meanX) ** 2;
  }
  return denominator === 0 ? 0 : numerator / denominator;
}

/** Population standard deviation. Returns 0 for fewer than two points. */
export function standardDeviation(values: number[]): number {
  const n = values.length;
  if (n < 2) return 0;
  const mean = values.reduce((sum, v) => sum + v, 0) / n;
  const variance = values.reduce((sum, v) => sum + (v - mean) ** 2, 0) / n;
  return Math.sqrt(variance);
}

/** Classifies a change, treating movement inside the stable band as flat. */
export function classifyDirection(change: number): TrendDirection {
  if (change > STABLE_BAND) return 'improving';
  if (change < -STABLE_BAND) return 'declining';
  return 'stable';
}

/** The trend a window with nothing in it produces. */
function emptyTrend(sampleSize: number): HealthScoreTrend {
  return {
    sampleSize,
    direction: 'stable',
    change: 0,
    slope: 0,
    volatility: 0,
    average: 0,
    first: 0,
    last: 0,
    movers: [],
    projectedNext: null,
    windowStart: null,
    windowEnd: null,
  };
}

/** Per-component movement across the window, largest absolute change first. */
export function componentMovements(
  points: HealthScoreHistoryPoint[],
): ComponentMovement[] {
  if (points.length < 2) return [];
  const first = points[0].components;
  const last = points[points.length - 1].components;

  return COMPONENT_KEYS.map((component) => ({
    component,
    first: round(first[component] ?? 0),
    last: round(last[component] ?? 0),
    change: round((last[component] ?? 0) - (first[component] ?? 0)),
  })).sort((a, b) => Math.abs(b.change) - Math.abs(a.change));
}

/** Computes the trend over an explicit set of history points. */
export function computeTrend(points: HealthScoreHistoryPoint[]): HealthScoreTrend {
  if (points.length === 0) return emptyTrend(0);

  const scores = points.map((p) => p.overallScore);
  const first = scores[0];
  const last = scores[scores.length - 1];

  if (points.length === 1) {
    return {
      ...emptyTrend(1),
      average: round(first),
      first: round(first),
      last: round(last),
      windowStart: points[0].timestamp,
      windowEnd: points[0].timestamp,
    };
  }

  const change = last - first;
  const slope = leastSquaresSlope(scores);
  const average = scores.reduce((sum, v) => sum + v, 0) / scores.length;

  return {
    sampleSize: points.length,
    direction: classifyDirection(change),
    change: round(change),
    slope: round(slope),
    volatility: round(standardDeviation(scores)),
    average: round(average),
    first: round(first),
    last: round(last),
    movers: componentMovements(points),
    projectedNext: clampScore(last + slope),
    windowStart: points[0].timestamp,
    windowEnd: points[points.length - 1].timestamp,
  };
}

export class ProtocolHealthTrendService {
  /**
   * Trend over the most recent `window` recorded scores.
   *
   * @param window how many history points to include (default 30, min 2)
   */
  getTrend(window = 30): HealthScoreTrend {
    const size = Math.max(2, Math.floor(window));
    const history = protocolHealthScoreService.getHistory(size);
    return computeTrend(history);
  }
}

export const protocolHealthTrendService = new ProtocolHealthTrendService();
