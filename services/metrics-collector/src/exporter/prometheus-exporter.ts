import http from 'node:http';
import type { ProtocolMetricSample } from '../types.js';
import type { MetricsRepository } from '../db/repository.js';
import type { Logger } from '../utils/logger.js';

const METRIC_DEFS: Array<{
  name: string;
  help: string;
  type: 'gauge' | 'counter';
  value: (sample: ProtocolMetricSample) => number;
}> = [
  { name: 'stellarlend_tvl', help: 'Total value locked in the protocol.', type: 'gauge', value: (s) => s.tvl },
  {
    name: 'stellarlend_total_borrows',
    help: 'Total outstanding borrows across the protocol.',
    type: 'gauge',
    value: (s) => s.totalBorrows,
  },
  {
    name: 'stellarlend_utilization_rate',
    help: 'Protocol-wide utilization rate (borrows / deposits).',
    type: 'gauge',
    value: (s) => s.utilizationRate,
  },
  {
    name: 'stellarlend_liquidations_total',
    help: 'Number of liquidations recorded at the latest sample.',
    type: 'counter',
    value: (s) => s.liquidations,
  },
  {
    name: 'stellarlend_total_deposits',
    help: 'Total deposits across the protocol.',
    type: 'gauge',
    value: (s) => s.totalDeposits,
  },
  { name: 'stellarlend_active_users', help: 'Active users at the latest sample.', type: 'gauge', value: (s) => s.activeUsers },
];

/**
 * Formats the latest protocol metric sample as Prometheus text exposition format.
 * https://prometheus.io/docs/instrumenting/exposition_formats/
 */
export function formatProtocolMetricsAsPrometheus(sample: ProtocolMetricSample | null): string {
  if (!sample) return '';
  const lines: string[] = [];
  for (const def of METRIC_DEFS) {
    lines.push(`# HELP ${def.name} ${def.help}`);
    lines.push(`# TYPE ${def.name} ${def.type}`);
    lines.push(`${def.name} ${def.value(sample)}`);
  }
  return lines.join('\n') + '\n';
}

/**
 * Starts a minimal HTTP server exposing the latest protocol metrics at GET /metrics
 * in Prometheus text-exposition format, for scraping.
 */
export function startPrometheusExporter(
  repo: MetricsRepository,
  port: number,
  logger: Logger
): http.Server {
  const server = http.createServer((req, res) => {
    if (req.url !== '/metrics') {
      res.writeHead(404).end('Not found');
      return;
    }
    repo
      .getLatestProtocolSample()
      .then((sample) => {
        res.writeHead(200, { 'Content-Type': 'text/plain; version=0.0.4; charset=utf-8' });
        res.end(formatProtocolMetricsAsPrometheus(sample));
      })
      .catch((err) => {
        logger.error('Failed to serve /metrics', err);
        res.writeHead(500).end('Internal Server Error');
      });
  });

  server.listen(port, () => {
    logger.info(`Prometheus exporter listening on :${port}/metrics`);
  });

  return server;
}
