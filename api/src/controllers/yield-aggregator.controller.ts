import { Request, Response, NextFunction } from 'express';
import { yieldAggregatorService } from '../services/yield-aggregator.service';
import logger from '../utils/logger';

export class YieldAggregatorController {
  /**
   * GET /api/yield-aggregator/pools
   * List all aggregated lending pools.
   */
  async getPools(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { asset } = req.query;
      const pools = yieldAggregatorService.getAllPools(asset as string | undefined);
      res.status(200).json({ success: true, data: pools });
    } catch (error) {
      logger.error('Error fetching pools:', error);
      next(error);
    }
  }

  /**
   * POST /api/yield-aggregator/route
   * Find best-rate routing for asset and deposit amount.
   */
  async getBestRateRoute(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { asset, depositAmount, strategy, maxSplits } = req.body;
      if (!asset || typeof depositAmount !== 'number' || depositAmount <= 0) {
        res.status(400).json({
          success: false,
          error: 'Asset and positive depositAmount are required',
        });
        return;
      }

      const route = yieldAggregatorService.findBestRateRoute(
        asset,
        depositAmount,
        strategy || 'highest_yield',
        maxSplits ? Number(maxSplits) : 3
      );
      res.status(200).json({ success: true, data: route });
    } catch (error) {
      logger.error('Error calculating best-rate route:', error);
      next(error);
    }
  }

  /**
   * POST /api/yield-aggregator/compare
   * Compare multiple pools side by side.
   */
  async comparePools(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { poolIds, asset } = req.body;
      const comparison = yieldAggregatorService.comparePools(poolIds, asset);
      res.status(200).json({ success: true, data: comparison });
    } catch (error) {
      logger.error('Error comparing pools:', error);
      next(error);
    }
  }

  /**
   * GET /api/yield-aggregator/analytics/:poolId
   * Fetch historical yield analytics and utilization curve for a pool.
   */
  async getPoolAnalytics(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { poolId } = req.params;
      const analytics = yieldAggregatorService.getYieldAnalytics(poolId);
      res.status(200).json({ success: true, data: analytics });
    } catch (error) {
      logger.error('Error fetching pool analytics:', error);
      next(error);
    }
  }

  /**
   * POST /api/yield-aggregator/alerts
   * Create a yield alert notification subscription.
   */
  async createAlert(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { userId, asset, targetApy, condition } = req.body;
      if (!userId || !asset || typeof targetApy !== 'number') {
        res.status(400).json({
          success: false,
          error: 'userId, asset, and numeric targetApy are required',
        });
        return;
      }

      const alert = yieldAggregatorService.createAlert(
        userId,
        asset,
        targetApy,
        condition || 'above'
      );
      res.status(201).json({ success: true, data: alert });
    } catch (error) {
      logger.error('Error creating yield alert:', error);
      next(error);
    }
  }

  /**
   * GET /api/yield-aggregator/alerts/:userId
   * Retrieve active yield alerts for a user.
   */
  async getUserAlerts(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { userId } = req.params;
      const alerts = yieldAggregatorService.getUserAlerts(userId);
      res.status(200).json({ success: true, data: alerts });
    } catch (error) {
      logger.error('Error fetching user alerts:', error);
      next(error);
    }
  }

  /**
   * DELETE /api/yield-aggregator/alerts/:id
   * Delete an alert subscription.
   */
  async deleteAlert(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { id } = req.params;
      const deleted = yieldAggregatorService.deleteAlert(id);
      res.status(200).json({ success: true, data: { deleted } });
    } catch (error) {
      logger.error('Error deleting alert:', error);
      next(error);
    }
  }

  /**
   * GET /api/yield-aggregator/alerts/check
   * Trigger alert evaluation across all active subscriptions.
   */
  async checkAlerts(_req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const triggered = yieldAggregatorService.checkAlerts();
      res.status(200).json({ success: true, data: { triggeredCount: triggered.length, triggered } });
    } catch (error) {
      logger.error('Error checking alerts:', error);
      next(error);
    }
  }
}

export const yieldAggregatorController = new YieldAggregatorController();
