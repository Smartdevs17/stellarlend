/**
 * Protocol Health Score Controller — Issue #692
 *
 * Serves the composite health score, its history, its trend and its alerts
 * under `/api/health`, alongside the liveness and readiness probes that already
 * live there. The scoring itself belongs to
 * `services/protocol-health/healthScore.service`; trending belongs to
 * `healthTrend.service`. This controller only validates input and shapes
 * responses.
 */

import { Request, Response } from 'express';
import { protocolHealthScoreService } from '../services/protocol-health/healthScore.service';
import { protocolHealthTrendService } from '../services/protocol-health/healthTrend.service';
import { ValidationError } from '../utils/errors';
import logger from '../utils/logger';

/** Largest history window a caller may request, to bound the response. */
const MAX_WINDOW = 365;

/**
 * Parses a positive-integer query parameter.
 *
 * @throws ValidationError when present but not a positive integer within `max`
 */
function parseWindow(raw: unknown, name: string, max: number): number | undefined {
  if (raw === undefined || raw === null || raw === '') return undefined;
  const value = Number(raw);
  if (!Number.isInteger(value) || value <= 0) {
    throw new ValidationError(`${name} must be a positive integer`);
  }
  if (value > max) {
    throw new ValidationError(`${name} must not exceed ${max}`);
  }
  return value;
}

export class HealthScoreController {
  /** GET /api/health/score — the current composite score and its components. */
  async getScore(_req: Request, res: Response): Promise<void> {
    try {
      const score = await protocolHealthScoreService.getHealthScore();
      res.json({ success: true, data: score });
    } catch (error) {
      HealthScoreController.fail(res, error, 'Failed to compute protocol health score');
    }
  }

  /** GET /api/health/score/history?limit=30 */
  async getHistory(req: Request, res: Response): Promise<void> {
    try {
      const limit = parseWindow(req.query.limit, 'limit', MAX_WINDOW);
      const history = protocolHealthScoreService.getHistory(limit);
      res.json({ success: true, data: history, meta: { count: history.length } });
    } catch (error) {
      HealthScoreController.fail(res, error, 'Failed to fetch health score history');
    }
  }

  /** GET /api/health/score/trend?window=30 */
  async getTrend(req: Request, res: Response): Promise<void> {
    try {
      const window = parseWindow(req.query.window, 'window', MAX_WINDOW) ?? 30;
      const trend = protocolHealthTrendService.getTrend(window);
      res.json({ success: true, data: trend });
    } catch (error) {
      HealthScoreController.fail(res, error, 'Failed to compute health score trend');
    }
  }

  /**
   * GET /api/health/score/alerts
   *
   * Returns 200 with an empty array when nothing has breached — an alerts
   * endpoint that errors when all is well is useless for monitoring.
   */
  async getAlerts(_req: Request, res: Response): Promise<void> {
    try {
      const alerts = await protocolHealthScoreService.getAlerts();
      res.json({
        success: true,
        data: alerts,
        meta: {
          threshold: protocolHealthScoreService.getAlertThreshold(),
          count: alerts.length,
        },
      });
    } catch (error) {
      HealthScoreController.fail(res, error, 'Failed to fetch health score alerts');
    }
  }

  /**
   * GET /api/health/score/summary
   *
   * Score, trend and alerts in one response, so a dashboard renders from a
   * single request instead of three that can disagree with each other.
   */
  async getSummary(req: Request, res: Response): Promise<void> {
    try {
      const window = parseWindow(req.query.window, 'window', MAX_WINDOW) ?? 30;
      const [score, alerts] = await Promise.all([
        protocolHealthScoreService.getHealthScore(),
        protocolHealthScoreService.getAlerts(),
      ]);
      const trend = protocolHealthTrendService.getTrend(window);

      res.json({
        success: true,
        data: {
          score,
          trend,
          alerts,
          threshold: protocolHealthScoreService.getAlertThreshold(),
        },
      });
    } catch (error) {
      HealthScoreController.fail(res, error, 'Failed to build health score summary');
    }
  }

  /** PUT /api/health/score/weights — governance/admin-controlled. */
  async updateWeights(req: Request, res: Response): Promise<void> {
    try {
      const partial = req.body ?? {};
      if (typeof partial !== 'object' || Array.isArray(partial)) {
        throw new ValidationError('Request body must be an object of component weights');
      }
      const updated = protocolHealthScoreService.updateWeights(partial);
      res.json({ success: true, data: updated });
    } catch (error) {
      HealthScoreController.fail(res, error, 'Failed to update health score weights');
    }
  }

  /** GET /api/health/score/weights */
  async getWeights(_req: Request, res: Response): Promise<void> {
    try {
      res.json({ success: true, data: protocolHealthScoreService.getWeights() });
    } catch (error) {
      HealthScoreController.fail(res, error, 'Failed to fetch health score weights');
    }
  }

  /** PUT /api/health/score/alert-threshold — governance/admin-controlled. */
  async updateAlertThreshold(req: Request, res: Response): Promise<void> {
    try {
      const { threshold } = req.body ?? {};
      if (threshold === undefined || threshold === null || Number.isNaN(Number(threshold))) {
        throw new ValidationError('threshold is required and must be a number');
      }
      protocolHealthScoreService.setAlertThreshold(Number(threshold));
      res.json({
        success: true,
        data: { threshold: protocolHealthScoreService.getAlertThreshold() },
      });
    } catch (error) {
      HealthScoreController.fail(res, error, 'Failed to update health score alert threshold');
    }
  }

  private static fail(res: Response, error: unknown, fallbackMessage: string): void {
    if (error instanceof ValidationError) {
      res.status(400).json({ success: false, error: error.message });
      return;
    }
    logger.error(fallbackMessage, error);
    res.status(500).json({ success: false, error: fallbackMessage });
  }
}

export const healthScoreController = new HealthScoreController();
