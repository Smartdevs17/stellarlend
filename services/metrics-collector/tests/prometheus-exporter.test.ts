import { describe, expect, it } from 'vitest';
import { formatProtocolMetricsAsPrometheus } from '../src/exporter/prometheus-exporter.js';
import type { ProtocolMetricSample } from '../src/types.js';

describe('formatProtocolMetricsAsPrometheus', () => {
  it('returns empty string when there is no sample yet', () => {
    expect(formatProtocolMetricsAsPrometheus(null)).toBe('');
  });

  it('formats a sample as Prometheus text-exposition format', () => {
    const sample: ProtocolMetricSample = {
      time: new Date('2026-01-01T00:00:00Z'),
      tvl: 1_000_000,
      totalBorrows: 500_000,
      utilizationRate: 0.5,
      liquidations: 3,
      totalDeposits: 1_200_000,
      activeUsers: 42,
    };

    const text = formatProtocolMetricsAsPrometheus(sample);

    expect(text).toContain('# HELP stellarlend_tvl Total value locked in the protocol.');
    expect(text).toContain('# TYPE stellarlend_tvl gauge');
    expect(text).toContain('stellarlend_tvl 1000000');
    expect(text).toContain('# TYPE stellarlend_liquidations_total counter');
    expect(text).toContain('stellarlend_liquidations_total 3');
    expect(text).toContain('stellarlend_active_users 42');
  });
});
