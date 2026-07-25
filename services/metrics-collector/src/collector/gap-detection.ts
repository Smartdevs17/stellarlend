import type { MetricsGap } from '../types.js';

/**
 * Detect missing collection intervals given a sorted list of sample timestamps
 * and the expected cadence (e.g. 60_000 ms).
 */
export function detectGaps(
  timestamps: Date[],
  intervalMs: number,
  metricFamily: MetricsGap['metricFamily'] = 'protocol'
): MetricsGap[] {
  if (timestamps.length < 2 || intervalMs <= 0) return [];

  const sorted = [...timestamps].sort((a, b) => a.getTime() - b.getTime());
  const gaps: MetricsGap[] = [];
  const tolerance = intervalMs * 1.5;

  for (let i = 1; i < sorted.length; i++) {
    const prev = sorted[i - 1]!;
    const curr = sorted[i]!;
    const delta = curr.getTime() - prev.getTime();
    if (delta > tolerance) {
      gaps.push({
        metricFamily,
        gapStart: new Date(prev.getTime() + intervalMs),
        gapEnd: new Date(curr.getTime() - intervalMs),
      });
    }
  }

  return gaps;
}

/**
 * Generate backfill timestamps for a detected gap at the given cadence.
 */
export function backfillTimestamps(gap: MetricsGap, intervalMs: number): Date[] {
  const points: Date[] = [];
  let cursor = gap.gapStart.getTime();
  const end = gap.gapEnd.getTime();
  while (cursor <= end) {
    points.push(new Date(cursor));
    cursor += intervalMs;
  }
  return points;
}
