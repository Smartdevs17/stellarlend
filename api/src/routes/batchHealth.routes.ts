import { Router } from 'express';
import { StellarService } from '../services/stellar.service';
import { redisCacheService } from '../services/redisCache.service';
import logger from '../utils/logger';

const router = Router();

const BATCH_HEALTH_TTL = 30; // 30 seconds cache TTL

const batchController = {
  healthCheck: async (req: any, res: any) => {
    try {
      const { queries, offset = 0, limit = 20 } = req.body;
      if (!Array.isArray(queries) || queries.length === 0) {
        return res.status(400).json({ error: 'queries must be a non-empty array' });
      }

      const validQueries = queries.filter(
        (q: any) => q.pool && q.user && q.asset
      );

      const pagedQueries = validQueries.slice(offset, offset + limit);

      const stellarService = new StellarService();
      
      // Extract unique pools to batch read
      const uniquePools = Array.from(new Set(pagedQueries.map((q: any) => q.pool)));
      
      // 1. Batched pool state reading (as required by #711)
      const poolStates = await stellarService.getMultiplePoolStates(uniquePools);
      const poolMap = new Map(poolStates.map(p => [p.pool, p]));

      // 2. Fetch or compute individual position healths using Redis cache
      const results = await Promise.all(pagedQueries.map(async (q: any) => {
        const cacheKey = redisCacheService.buildKey('position', `${q.user}:${q.pool}`);
        let healthData = await redisCacheService.get<any>(cacheKey);

        if (!healthData) {
          // If not in cache, fallback to computing / fetching (Mocked for now)
          const poolState = poolMap.get(q.pool);
          const minRatio = poolState ? Number(poolState.minCollateralRatioBps) / 10000 : 1.5;
          
          healthData = {
            collateral_balance: 1000,
            collateral_value: 1000,
            debt_balance: 500,
            debt_value: 500,
            health_factor: 20000, // 2.0x
            is_liquidatable: false,
            max_liquidatable: 0,
            success: true
          };

          // Cache the health data to prevent RPC spam
          await redisCacheService.set(cacheKey, healthData, BATCH_HEALTH_TTL);
        }

        return {
          pool: q.pool,
          user: q.user,
          asset: q.asset,
          ...healthData
        };
      }));

      const healthy = results.filter((r: any) => !r.is_liquidatable).length;
      let totalHealth = 0;
      results.forEach(r => totalHealth += r.health_factor);

      res.json({
        results,
        total_positions: results.length,
        healthy_positions: healthy,
        liquidatable_positions: results.length - healthy,
        avg_health_factor: results.length > 0 ? Math.floor(totalHealth / results.length) : 0,
      });
    } catch (error) {
      logger.error('Failed to run batch health check', { error });
      res.status(500).json({ error: 'Failed to run batch health check' });
    }
  },

  getTotalValue: async (req: any, res: any) => {
    try {
      const { queries } = req.body;
      if (!Array.isArray(queries) || queries.length === 0) {
        return res.status(400).json({ error: 'queries must be a non-empty array' });
      }
      res.json({ total_collateral: 0, total_debt: 0 });
    } catch (error) {
      logger.error('Failed to compute total batch value', { error });
      res.status(500).json({ error: 'Failed to compute total batch value' });
    }
  },

  getLiquidatable: async (req: any, res: any) => {
    try {
      const { queries } = req.body;
      if (!Array.isArray(queries) || queries.length === 0) {
        return res.status(400).json({ error: 'queries must be a non-empty array' });
      }
      res.json([]);
    } catch (error) {
      logger.error('Failed to get liquidatable positions', { error });
      res.status(500).json({ error: 'Failed to get liquidatable positions' });
    }
  },
};

router.post('/health-check', batchController.healthCheck);
router.post('/total-value', batchController.getTotalValue);
router.post('/liquidatable', batchController.getLiquidatable);

export default router;
