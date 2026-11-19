/**
 * Pool-utilization materialized-view controller (Issue #1650).
 *
 * Thin HTTP handler layer — all business logic lives in
 * poolUtilizationMv.service.ts.
 */

import { Request, Response, NextFunction } from 'express';
import {
  getPoolUtilizationTimeSeries,
  getAllPoolsUtilizationTimeSeries,
  refreshPoolUtilizationTimeSeries,
} from '../services/poolUtilizationMv.service';

/**
 * GET /api/pool-performance/utilization-timeseries/:poolAddress
 *
 * Query params:
 *   period  — '7d' | '30d' | '90d' | '1y'  (default: '30d')
 *
 * Returns hourly utilization buckets for the requested pool and period,
 * served from the pool_utilization_timeseries materialised view.
 */
export const getUtilizationTimeSeries = async (
  req: Request,
  res: Response,
  next: NextFunction,
): Promise<void> => {
  try {
    const { poolAddress } = req.params;
    const period = (req.query['period'] as string) || '30d';

    if (!poolAddress || typeof poolAddress !== 'string') {
      res.status(400).json({ success: false, error: 'poolAddress path parameter is required' });
      return;
    }

    const result = await getPoolUtilizationTimeSeries(poolAddress, period);
    res.status(200).json({ success: true, data: result });
  } catch (error) {
    next(error);
  }
};

/**
 * GET /api/pool-performance/utilization-timeseries
 *
 * Query params:
 *   period  — '7d' | '30d' | '90d' | '1y'  (default: '30d')
 *
 * Returns hourly utilization buckets for all pools over the requested period.
 */
export const getAllUtilizationTimeSeries = async (
  req: Request,
  res: Response,
  next: NextFunction,
): Promise<void> => {
  try {
    const period = (req.query['period'] as string) || '30d';
    const buckets = await getAllPoolsUtilizationTimeSeries(period);
    res.status(200).json({ success: true, data: buckets });
  } catch (error) {
    next(error);
  }
};

/**
 * POST /api/pool-performance/utilization-timeseries/refresh
 *
 * Triggers a manual CONCURRENTLY refresh of pool_utilization_timeseries.
 * Protected by API-key auth in production (applied at the router level).
 */
export const manualRefreshUtilizationTimeSeries = async (
  _req: Request,
  res: Response,
  next: NextFunction,
): Promise<void> => {
  try {
    const result = await refreshPoolUtilizationTimeSeries();
    res.status(200).json({ success: true, data: result });
  } catch (error) {
    next(error);
  }
};
