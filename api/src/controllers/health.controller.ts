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
import { portfolioHealthHeatmapService } from '../services/protocol-health/portfolioHealthHeatmap.service';
import { ValidationError } from '../utils/errors';
import logger from '../utils/logger';

/** Largest history window a caller may request, to bound the response. */
const MAX_WINDOW = 365;

/** Bounds for the simulated shock grid, so a caller cannot request an unbounded matrix. */
const MAX_SHOCK_SCENARIOS = 20;
const MAX_SHOCK_ASSETS = 50;
const MIN_SHOCK_MAGNITUDE = -100;
const MAX_SHOCK_MAGNITUDE = 100;

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

  /**
   * POST /api/health/heatmap/simulate
   *
   * Builds the portfolio health heatmap under simulated market shocks. The
   * dashboard sends the shock scenarios it wants applied; the response is a
   * grid of per-asset health deltas plus the aggregate impact per scenario.
   *
   * Body:
   *   {
   *     scenarios?: Array<{ id?: string; label?: string; shocks: Record<string, number> }>,
   *     assets?: string[],
   *     magnitude?: number
   *   }
   *
   * When `scenarios` is omitted the service's default shock set is used.
   */
  async simulateHeatmap(req: Request, res: Response): Promise<void> {
    const startedAt = Date.now();
    try {
      const body = req.body ?? {};
      if (typeof body !== 'object' || Array.isArray(body)) {
        throw new ValidationError('Request body must be an object');
      }

      const scenarios = HealthScoreController.parseScenarios(body.scenarios);
      const assets = HealthScoreController.parseAssets(body.assets);
      const magnitude = HealthScoreController.parseMagnitude(body.magnitude);

      const heatmap = await portfolioHealthHeatmapService.simulate({
        scenarios,
        assets,
        magnitude,
      });

      logger.info('Portfolio health heatmap simulated', {
        scenarioCount: heatmap.scenarios.length,
        assetCount: heatmap.assets.length,
        durationMs: Date.now() - startedAt,
      });

      res.json({
        success: true,
        data: heatmap,
        meta: {
          scenarioCount: heatmap.scenarios.length,
          assetCount: heatmap.assets.length,
          durationMs: Date.now() - startedAt,
        },
      });
    } catch (error) {
      logger.warn('Portfolio health heatmap simulation failed', {
        durationMs: Date.now() - startedAt,
        reason: error instanceof Error ? error.message : String(error),
      });
      HealthScoreController.fail(res, error, 'Failed to simulate portfolio health heatmap');
    }
  }

  /** Validates and normalises the optional shock scenario list. */
  private static parseScenarios(
    raw: unknown,
  ): Array<{ id?: string; label?: string; shocks: Record<string, number> }> | undefined {
    if (raw === undefined || raw === null) return undefined;
    if (!Array.isArray(raw)) {
      throw new ValidationError('scenarios must be an array');
    }
    if (raw.length === 0) {
      throw new ValidationError('scenarios must not be empty');
    }
    if (raw.length > MAX_SHOCK_SCENARIOS) {
      throw new ValidationError(`scenarios must not exceed ${MAX_SHOCK_SCENARIOS} entries`);
    }

    return raw.map((entry, index) => {
      if (typeof entry !== 'object' || entry === null || Array.isArray(entry)) {
        throw new ValidationError(`scenarios[${index}] must be an object`);
      }
      const { id, label, shocks } = entry as Record<string, unknown>;
      if (typeof shocks !== 'object' || shocks === null || Array.isArray(shocks)) {
        throw new ValidationError(`scenarios[${index}].shocks must be an object of asset shocks`);
      }
      const normalisedShocks: Record<string, number> = {};
      for (const [asset, value] of Object.entries(shocks as Record<string, unknown>)) {
        const numeric = Number(value);
        if (!Number.isFinite(numeric)) {
          throw new ValidationError(`scenarios[${index}].shocks.${asset} must be a number`);
        }
        if (numeric < MIN_SHOCK_MAGNITUDE || numeric > MAX_SHOCK_MAGNITUDE) {
          throw new ValidationError(
            `scenarios[${index}].shocks.${asset} must be between ${MIN_SHOCK_MAGNITUDE} and ${MAX_SHOCK_MAGNITUDE}`,
          );
        }
        normalisedShocks[asset] = numeric;
      }
      if (Object.keys(normalisedShocks).length === 0) {
        throw new ValidationError(`scenarios[${index}].shocks must not be empty`);
      }
      return {
        id: typeof id === 'string' ? id : undefined,
        label: typeof label === 'string' ? label : undefined,
        shocks: normalisedShocks,
      };
    });
  }

  /** Validates and normalises the optional asset filter. */
  private static parseAssets(raw: unknown): string[] | undefined {
    if (raw === undefined || raw === null) return undefined;
    if (!Array.isArray(raw)) {
      throw new ValidationError('assets must be an array of asset symbols');
    }
    if (raw.length === 0) {
      throw new ValidationError('assets must not be empty');
    }
    if (raw.length > MAX_SHOCK_ASSETS) {
      throw new ValidationError(`assets must not exceed ${MAX_SHOCK_ASSETS} entries`);
    }
    return raw.map((asset, index) => {
      if (typeof asset !== 'string' || asset.trim() === '') {
        throw new ValidationError(`assets[${index}] must be a non-empty string`);
      }
      return asset.trim();
    });
  }

  /** Validates the optional global shock magnitude. */
  private static parseMagnitude(raw: unknown): number | undefined {
    if (raw === undefined || raw === null) return undefined;
    const value = Number(raw);
    if (!Number.isFinite(value)) {
      throw new ValidationError('magnitude must be a number');
    }
    if (value < MIN_SHOCK_MAGNITUDE || value > MAX_SHOCK_MAGNITUDE) {
      throw new ValidationError(
        `magnitude must be between ${MIN_SHOCK_MAGNITUDE} and ${MAX_SHOCK_MAGNITUDE}`,
      );
    }
    return value;
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
