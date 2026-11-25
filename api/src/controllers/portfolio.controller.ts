import { Request, Response, NextFunction } from 'express';
import { StellarService } from '../services/stellar.service';
import {
  analyzePortfolio,
  toCSV,
  computeInterestAccrualProjection,
  computeLiquidationPrice,
  getHealthFactorMonitor as buildHealthFactorMonitor,
} from '../services/portfolio.service';
import {
  buildHealthHeatmap,
  DEFAULT_SHOCK_SCENARIOS,
  MAX_SHOCK_SCENARIOS,
  ShockScenario,
} from '../services/position-health-sim.service';
import { PortfolioAnalyticsResponse } from '../types/portfolio';
import { redisCacheService } from '../services/redisCache.service';
import { config } from '../config/index';
import { logger } from '../utils/logger';

const PORTFOLIO_CACHE_TTL_S = Math.floor(config.cache.positionTtlMs / 1000);
const HEATMAP_CACHE_TTL_S = Math.max(1, Math.min(PORTFOLIO_CACHE_TTL_S, 60));

const MAX_SHOCK_PCT = 100;

/**
 * Parses and validates the `shocks` query parameter.
 *
 * Accepted forms:
 *   - omitted / empty  -> DEFAULT_SHOCK_SCENARIOS
 *   - JSON array       -> [{"label":"crash","priceChangePct":-30}, ...]
 *   - CSV list         -> "-30,-10,0,10" (labels are derived from the pct)
 *
 * Throws a 400-tagged error on malformed input so the error middleware can
 * surface a client error rather than a 500.
 */
const parseShockScenarios = (raw: unknown): ShockScenario[] => {
  if (raw === undefined || raw === null || raw === '') {
    return DEFAULT_SHOCK_SCENARIOS;
  }

  if (typeof raw !== 'string') {
    throw Object.assign(new Error('Invalid "shocks" parameter'), { statusCode: 400 });
  }

  let scenarios: ShockScenario[];

  if (raw.trim().startsWith('[')) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      throw Object.assign(new Error('"shocks" must be valid JSON'), { statusCode: 400 });
    }
    if (!Array.isArray(parsed)) {
      throw Object.assign(new Error('"shocks" must be a JSON array'), { statusCode: 400 });
    }
    scenarios = parsed.map((entry, index) => {
      if (typeof entry === 'number') {
        return { label: `${entry}%`, priceChangePct: entry };
      }
      if (!entry || typeof entry !== 'object') {
        throw Object.assign(new Error(`Invalid shock at index ${index}`), { statusCode: 400 });
      }
      const { label, priceChangePct } = entry as Record<string, unknown>;
      if (typeof priceChangePct !== 'number' || !Number.isFinite(priceChangePct)) {
        throw Object.assign(
          new Error(`Shock at index ${index} requires a numeric priceChangePct`),
          { statusCode: 400 }
        );
      }
      return {
        label: typeof label === 'string' && label.trim() ? label.trim() : `${priceChangePct}%`,
        priceChangePct,
      };
    });
  } else {
    scenarios = raw
      .split(',')
      .map((part) => part.trim())
      .filter((part) => part.length > 0)
      .map((part) => {
        const value = Number(part);
        if (!Number.isFinite(value)) {
          throw Object.assign(new Error(`Invalid shock value "${part}"`), { statusCode: 400 });
        }
        return { label: `${value}%`, priceChangePct: value };
      });
  }

  if (scenarios.length === 0) {
    throw Object.assign(new Error('"shocks" must contain at least one scenario'), {
      statusCode: 400,
    });
  }
  if (scenarios.length > MAX_SHOCK_SCENARIOS) {
    throw Object.assign(
      new Error(`"shocks" may contain at most ${MAX_SHOCK_SCENARIOS} scenarios`),
      { statusCode: 400 }
    );
  }
  for (const scenario of scenarios) {
    if (Math.abs(scenario.priceChangePct) > MAX_SHOCK_PCT) {
      throw Object.assign(
        new Error(`Shock "${scenario.label}" exceeds ±${MAX_SHOCK_PCT}%`),
        { statusCode: 400 }
      );
    }
  }

  return scenarios;
};

export const getPortfolioAnalytics = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const { userAddress } = req.params!;
    const cacheKey = redisCacheService.buildKey('position', `portfolio:${userAddress}`);

    const cached = await redisCacheService.get<PortfolioAnalyticsResponse>(cacheKey);
    if (cached) {
      res.status(200).json(cached);
      return;
    }

    const stellarService = new StellarService();
    const [position, history] = await Promise.all([
      stellarService.getUserPosition(userAddress!),
      stellarService.getTransactionHistory({ userAddress: userAddress!, limit: 200 }),
    ]);

    const analytics = analyzePortfolio(userAddress!, position, history.data);

    await redisCacheService.set(cacheKey, analytics, PORTFOLIO_CACHE_TTL_S);

    res.status(200).json(analytics);
  } catch (error) {
    next(error);
    return;
  }
};

export const getPortfolioRisk = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const { userAddress } = req.params!;
    const stellarService = new StellarService();
    const position = await stellarService.getUserPosition(userAddress!);
    const analytics = analyzePortfolio(userAddress!, position!, []);

    res.status(200).json({
      userAddress,
      riskMetrics: analytics.riskMetrics,
      suggestions: analytics.suggestions,
      generatedAt: analytics.generatedAt,
    });
  } catch (error) {
    next(error);
    return;
  }
};

export const getPortfolioPerformance = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const { userAddress } = req.params!;
    const limit = req.query.limit ? Number(req.query.limit) : 200;

    const stellarService = new StellarService();
    const [position, history] = await Promise.all([
      stellarService.getUserPosition(userAddress!),
      stellarService.getTransactionHistory({ userAddress: userAddress!, limit }),
    ]);

    const analytics = analyzePortfolio(userAddress!, position, history.data);

    res.status(200).json({
      userAddress,
      performance: analytics.performance,
      pagination: history.pagination,
      generatedAt: analytics.generatedAt,
    });
  } catch (error) {
    next(error);
    return;
  }
};

export const exportPortfolio = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const { userAddress } = req.params!;
    const format = (req.query.format as string) ?? 'json';

    const stellarService = new StellarService();
    const [position, history] = await Promise.all([
      stellarService.getUserPosition(userAddress!),
      stellarService.getTransactionHistory({ userAddress: userAddress!, limit: 200 }),
    ]);

    const analytics = analyzePortfolio(userAddress!, position, history.data);

    if (format === 'csv') {
      res.setHeader('Content-Type', 'text/csv');
      res.setHeader(
        'Content-Disposition',
        `attachment; filename="portfolio-${userAddress!.slice(0, 8)}.csv"`
      );
      res.status(200).send(toCSV(history.data));
      return;
    }

    res.setHeader('Content-Type', 'application/json');
    res.setHeader(
      'Content-Disposition',
      `attachment; filename="portfolio-${userAddress!.slice(0, 8)}.json"`
    );
    res.status(200).json({
      exportedAt: new Date().toISOString(),
      userAddress,
      analytics,
      transactions: history.data,
    });
  } catch (error) {
    next(error);
    return;
  }
};

export const getInterestProjection = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const { userAddress } = req.params!;
    const stellarService = new StellarService();
    const position = await stellarService.getUserPosition(userAddress!);
    const borrowApy = req.query.borrowApy ? Number(req.query.borrowApy) : 0.05;

    const projection = computeInterestAccrualProjection(position, borrowApy);
    res.status(200).json({
      userAddress,
      projection,
      generatedAt: new Date().toISOString(),
    });
  } catch (error) {
    next(error);
    return;
  }
};

export const getLiquidationPrice = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const { userAddress } = req.params!;
    const currentPrice = req.query.price ? Number(req.query.price) : 1;

    const stellarService = new StellarService();
    const position = await stellarService.getUserPosition(userAddress!);

    const liquidation = computeLiquidationPrice(position, currentPrice);
    res.status(200).json({
      userAddress,
      currentPrice,
      liquidation,
      generatedAt: new Date().toISOString(),
    });
  } catch (error) {
    next(error);
    return;
  }
};

export const getHealthFactorMonitor = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const { userAddress } = req.params!;

    const stellarService = new StellarService();
    const position = await stellarService.getUserPosition(userAddress!);

    const monitor = buildHealthFactorMonitor(position);
    res.status(200).json({
      userAddress,
      ...monitor,
      generatedAt: new Date().toISOString(),
    });
  } catch (error) {
    next(error);
    return;
  }
};

export const getPortfolioHealthHeatmap = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  const startedAt = Date.now();
  try {
    const { userAddress } = req.params!;
    if (!userAddress || !/^G[A-Z0-9]{55}$/.test(userAddress)) {
      res.status(400).json({ error: 'Invalid Stellar account address' });
      return;
    }

    const scenarios = parseShockScenarios(req.query.shocks);

    const cacheKey = redisCacheService.buildKey(
      'position',
      `health-heatmap:${userAddress}:${JSON.stringify(scenarios)}`
    );

    const cached = await redisCacheService.get<Record<string, unknown>>(cacheKey);
    if (cached) {
      logger.debug('portfolio.health_heatmap.cache_hit', { userAddress });
      res.status(200).json({ ...cached, cacheHit: true });
      return;
    }

    const stellarService = new StellarService();
    const position = await stellarService.getUserPosition(userAddress);

    const heatmap = buildHealthHeatmap(position, scenarios);

    const payload = {
      userAddress,
      scenarios,
      ...heatmap,
      generatedAt: new Date().toISOString(),
    };

    await redisCacheService.set(cacheKey, payload, HEATMAP_CACHE_TTL_S);

    logger.info('portfolio.health_heatmap.generated', {
      userAddress,
      scenarioCount: scenarios.length,
      assetCount: heatmap.assets.length,
      worstRiskLevel: heatmap.summary.worstRiskLevel,
      durationMs: Date.now() - startedAt,
    });

    res.status(200).json({ ...payload, cacheHit: false });
  } catch (error) {
    const statusCode = (error as { statusCode?: number }).statusCode;
    if (statusCode === 400) {
      logger.warn('portfolio.health_heatmap.invalid_request', {
        message: (error as Error).message,
      });
      res.status(400).json({ error: (error as Error).message });
      return;
    }
    logger.error('portfolio.health_heatmap.failed', {
      message: (error as Error).message,
      durationMs: Date.now() - startedAt,
    });
    next(error);
    return;
  }
};
