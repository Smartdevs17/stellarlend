import { Request, Response, NextFunction } from 'express';
import {
  isValidInterval,
  isValidMetric,
  queryTimeSeries,
  type MetricInterval,
  type MetricName,
} from '../services/metricsTimeseries.service';
import {
  metricsQueriesTotal,
  metricsQueryDuration,
  recordProtocolMetric,
  renderMetrics,
} from '../services/metrics.service';
import {
  getEventIndexer,
  formatIndexerMetricsAsJson,
  formatIndexerMetricsAsPrometheus,
} from '../services/eventIndex';

/**
 * GET /api/metrics/timeseries?metric=&from=&to=&interval=
 */
export const getTimeSeries = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  const metric = String(req.query.metric ?? '');
  const interval = String(req.query.interval ?? '1h');
  const startAt = process.hrtime.bigint();

  try {
    const fromRaw = String(req.query.from ?? '');
    const toRaw = String(req.query.to ?? '');

    if (!isValidMetric(metric)) {
      metricsQueriesTotal.labels(metric || 'unknown', interval, 'invalid_metric').inc();
      res.status(400).json({
        error: 'Invalid metric',
        allowed: [
          'tvl',
          'totalBorrows',
          'utilizationRate',
          'liquidations',
          'totalDeposits',
          'activeUsers',
        ],
      });
      return;
    }

    if (!isValidInterval(interval)) {
      metricsQueriesTotal.labels(metric, interval, 'invalid_interval').inc();
      res.status(400).json({
        error: 'Invalid interval',
        allowed: ['1m', '5m', '1h', '1d'],
      });
      return;
    }

    const from = fromRaw ? new Date(fromRaw) : new Date(Date.now() - 24 * 60 * 60 * 1000);
    const to = toRaw ? new Date(toRaw) : new Date();

    if (Number.isNaN(from.getTime()) || Number.isNaN(to.getTime())) {
      metricsQueriesTotal.labels(metric, interval, 'invalid_range').inc();
      res.status(400).json({ error: 'from and to must be valid ISO timestamps' });
      return;
    }

    if (from > to) {
      metricsQueriesTotal.labels(metric, interval, 'invalid_range').inc();
      res.status(400).json({ error: 'from must be <= to' });
      return;
    }

    const result = await queryTimeSeries({
      metric: metric as MetricName,
      from,
      to,
      interval: interval as MetricInterval,
    });

    const latest = result.points[result.points.length - 1];
    if (latest && typeof latest.value === 'number') {
      recordProtocolMetric(metric, latest.value);
    }

    metricsQueriesTotal.labels(metric, interval, 'success').inc();
    res.status(200).json(result);
  } catch (error) {
    metricsQueriesTotal.labels(metric || 'unknown', interval, 'error').inc();
    next(error);
  } finally {
    const durationSeconds = Number(process.hrtime.bigint() - startAt) / 1000000000;
    metricsQueryDuration.labels(metric || 'unknown', interval).observe(durationSeconds);
  }
};

/**
 * GET /api/metrics/prometheus
 * Exposes Prometheus-scrapable text format metrics.
 */
export const getPrometheusMetrics = async (
  _req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const body = await renderMetrics();
    res.setHeader('Content-Type', 'text/plain; version=0.0.4; charset=utf-8');
    res.status(200).send(body);
  } catch (error) {
    next(error);
  }
};

/**
 * GET /api/metrics/indexer
 * Exposes indexer telemetry in Prometheus format or JSON
 */
export const getIndexerMetrics = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const indexer = getEventIndexer();
    const status = await indexer.status();
    const alerts = indexer.getAlerts();
    const accept = req.headers['accept'] || '';
    if (accept.includes('application/json') || req.query.format === 'json') {
      res.json(formatIndexerMetricsAsJson(status, alerts));
    } else {
      res.setHeader('Content-Type', 'text/plain; version=0.0.4; charset=utf-8');
      res.send(formatIndexerMetricsAsPrometheus(status, alerts));
    }
  } catch (error) {
    next(error);
  }
};