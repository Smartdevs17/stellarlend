import { Router, Request, Response, NextFunction } from 'express';
import { emergencyPauseService } from '../services/emergencyPause.service';
import { authenticateAdmin } from '../middleware/auth';
import logger from '../utils/logger';

const router: Router = Router();

interface AssetShutdownState {
  assetAddress: string;
  is_active: boolean;
  reason: string;
  started_at: number;
  resolved_at: number | null;
  triggered_by: string;
}

// Per-asset shutdown registry. Keyed by normalized asset address.
const assetShutdowns: Map<string, AssetShutdownState> = new Map();

function normalizeAsset(assetAddress: unknown): string | null {
  if (typeof assetAddress !== 'string') return null;
  const trimmed = assetAddress.trim();
  if (!trimmed) return null;
  return trimmed.toLowerCase();
}

const emergencyController = {
  async getStatus(_req: Request, res: Response, next: NextFunction) {
    try {
      const state = emergencyPauseService.isPaused();
      const queue = emergencyPauseService.getWithdrawalQueue();
      const history = emergencyPauseService.getEmergencyHistory();
      ress.json({
        success: true,
        data: {
          ...state,
          queueLength: queue.length,
          queue,
          history,
          assetShutdowns: Array.from(assetShutdowns.values()),
        },
      });
    } catch (error) {
      next(error);
    }
  },

  async pause(req: Request, res: Response, next: NextFunction) {
    try {
      const { reason } = req.body;
      const pauseReason = reason || 'manual';
      emergencyPauseService.pause(pauseReason);
      logger.info(`Emergency pause triggered: ${pauseReason}`);
      res.json({ success: true, data: { paused: true, reason: pauseReason } });
    } catch (error) {
      next(error);
    }
  },

  async resume(_req: Request, res: Response, next: NextFunction) {
    try {
      emergencyPauseService.resume();
      logger.info('Emergency pause resumed');
      res.json({ success: true, data: { paused: false } });
    } catch (error) {
      next(error);
    }
  },

  // Scoped per-asset shutdown: only the targeted asset is blocked.
  async shutdownAsset(req: Request, res: Response, next: NextFunction) {
    try {
      const assetKey = normalizeAsset(req.body?.assetAddress ?? req.params.assetAddress);
      if (!assetKey) {
        return res.status(400).json({ success: false, error: 'assetAddress required' });
      }
      const existing = assetShutdowns.get(assetKey);
      if (existing && existing.is_active) {
        return res.status(400).json({ success: false, error: 'Asset already shut down', data: existing });
      }
      const now = Math.floor(Date.now() / 1000);
      const reason = typeof req.body?.reason === 'string' && req.body.reason.trim()
        ? req.body.reason.trim()
        : 'manual';
      const triggeredBy = typeof req.body?.triggeredBy === 'string' && req.body.triggeredBy.trim()
        ? req.body.triggeredBy.trim()
        : 'Admin';
      const state: AssetShutdownState = {
        assetAddress: assetKey,
        is_active: true,
        reason,
        started_at: now,
        resolved_at: null,
        triggered_by: triggeredBy,
      };
      assetShutdowns.set(assetKey, state);
      logger.info(`Asset shutdown triggered for ${assetKey}: ${reason}`);
      res.json({ success: true, data: state });
    } catch (error) {
      next(error);
    }
  },

  // Reversible: restores the asset to normal operation.
  async resumeAsset(req: Request, res: Response, next: NextFunction) {
    try {
      const assetKey = normalizeAsset(req.body?.assetAddress ?? req.params.assetAddress);
      if (!assetKey) {
        return res.status(400).json({ success: false, error: 'assetAddress required' });
      }
      const existing = assetShutdowns.get(assetKey);
      if (!existing || !existing.is_active) {
        return res.status(400).json({ success: false, error: 'Asset is not shut down' });
      }
      const now = Math.floor(Date.now() / 1000);
      const updated: AssetShutdownState = {
        ...existing,
        is_active: false,
        resolved_at: now,
      };
      assetShutdowns.set(assetKey, updated);
      logger.info(`Asset shutdown resolved for ${assetKey}`);
      res.json({ success: true, data: updated });
    } catch (error) {
      next(error);
    }
  },

  async getAssetShutdown(req: Request, res: Response, next: NextFunction) {
    try {
      const assetKey = normalizeAsset(req.params.assetAddress ?? req.query.assetAddress);
      if (!assetKey) {
        return res.status(400).json({ success: false, error: 'assetAddress required' });
      }
      const state = assetShutdowns.get(assetKey);
      if (!state) {
        return res.status(404).json({ success: false, error: 'Asset shutdown not found' });
      }
      res.json({ success: true, data: state });
    } catch (error) {
      next(error);
    }
  },

  async listAssetShutdowns(_req: Request, res: Response, next: NextFunction) {
    try {
      res.json({ success: true, data: Array.from(assetShutdowns.values()) });
    } catch (error) {
      next(error);
    }
  },

  async executeEmergencyWithdrawal(req: Request, res: Response, next: NextFunction) {
    try {
      const { userAddress, assetAddress, amount, tyHash } = req.body;
      if (!userAddress || amount === undefined) {
        return res.status(400).json({ success: false, error: 'userAddress and amount required' });
      }
      const assetKey = normalizeAsset(assetAddress);
      if (assetKey) {
        const shutdown = assetShutdowns.get(assetKey);
        if (shutdown && shutdown.is_active) {
          return res.status(400).json({ success: false, error: 'Asset is shut down' });
        }
      }
      const execution = emergencyPauseService.executeEmergencyWithdrawal({
        userAddress,
        assetAddress,
        amount: Number(amount),
        tyHash,
      });
      res.json({ success: true, data: execution });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Execution failed' });
    }
  },

  async previewFee(req: Request, res: Response, next: NextFunction) {
    try {
      const amount = Number(req.query.amount);
      if (isNaN(amount) || amount <= 0) {
        return res.status(400).json({ success: false, error: 'Valid amount query parameter required' });
      }
      const fee = emergencyPauseService.calculateEmergencyFee(amount);
      res.json({ success: true, data: fee });
    } catch (error) {
      next(error);
    }
  },

  async getLimits(_req: Request, res: Response, next: NextFunction) {
    try {
      const limits = emergencyPauseService.getLimits();
      res.json({ success: true, data: limits });
    } catch (error) {
      next(error);
    }
  },

  async updateLimits(req: Request, res: Response, next: NextFunction) {
    try {
      const updated = emergencyPauseService.updateLimits(req.body);
      res.json({ success: true, data: updated });
    } catch (error) {
      next(error);
    }
  },

  async getAnalytics(_req: Request, res: Response, next: NextFunction) {
    try {
      const analytics = emergencyPauseService.getEmergencyAnalytics();
      res.json({ success: true, data: analytics });
    } catch (error) {
      next(error);
    }
  },

  async getReport(_req: Request, res: Response, next: NextFunction) {
    try {
      const report = emergencyPauseService.generateEmergencyReport();
      res.json({ success: true, data: report });
    } catch (error) {
      next(error);
    }
  },

  async getWithdrawals(_req: Request, res: Response, next: NextFunction) {
    try {
      const withdrawals = emergencyPauseService.getEmergencyWithdrawals();
      res.json({ success: true, data: withdrawals });
    } catch (error) {
      next(error);
    }
  },

  async queueWithdrawal(req: Request, res: Response, next: NextFunction) {
    try {
      const { userAddress, assetAddress, amount } = req.body;
      if (!userAddress || !amount) {
        return res.status(400).json({ success: false, error: 'userAddress and amount required' });
      }
      const assetKey = normalizeAsset(assetAddress);
      if (assetKey) {
        const shutdown = assetShutdowns.get(assetKey);
        if (shutdown && shutdown.is_active) {
          return res.status(400).json({ success: false, error: 'Asset is shut down' });
        }
      }
      emergencyPauseService.queueWithdrawal({ userAddress, assetAddress, amount });
      const queue = emergencyPauseService.getWithdrawalQueue();
      res.json({ success: true, data: { queued: true, queueLength: queue.length } });
    } catch (error) {
      next(error);
    }
  },

  async drainQueue(_req: Request, res: Response, next: NextFunction) {
    try {
      const drained = emergencyPauseService.drainWithdrawalQueue();
      logger.info(`Emergency withdrawal queue drained: ${drained.length} entries`);
      res.json({ success: true, data: { drained, count: drained.length } });
    } catch (error) {
      next(error);
    }
  },

  async getQueue(_req: Request, res: Response, next: NextFunction) {
    try {
      const queue = emergencyPauseService.getWithdrawalQueue();
      res.json({ success: true, data: { queue, length: queue.length } });
    } catch (error) {
      next(error);
    }
  },

  async triggerFailure(_req: Request, res: Response, next: NextFunction) {
    try {
      emergencyPauseService.recordFailure();
      const state = emergencyPauseService.isPaused();
      res.json({ success: true, data: state });
    } catch (error) {
      next(error);
    }
  },

  async triggerSuccess(_req: Request, res: Response, next: NextFunction) {
    try {
      emergencyPauseService.recordSuccess();
      res.json({ success: true, data: { consecutiveFailures: 0 } });
    } catch (error) {
      next(error);
    }
  },

  async getNotifications(_req: Request, res: Response, next: NextFunction) {
    try {
      const notifications = emergencyPauseService.getNotifications();
      res.json({ success: true, data: notifications });
    } catch (error) {
      next(error);
    }
  },

  async getHistory(_req: Request, res: Response, next: NextFunction) {
    try {
      const history = emergencyPauseService.getEmergencyHistory();
      res.json({ success: true, data: history });
    } catch (error) {
      next(error);
    }
  },
};

router.get('/status', emergencyController.getStatus);
router.post('/pause', authenticateAdmin, emergencyController.pause);
router.post('/resume', authenticateAdmin, emergencyController.resume);
router.post('/withdraw', authenticateAdmin, emergencyController.executeEmergencyWithdrawal);
router.post('/emergency-withdraw', authenticateAdmin, emergencyController.executeEmergencyWithdrawal);
router.get('/fee-preview', emergencyController.previewFee);
router.get('/limits', emergencyController.getLimits);
router.put('/limits', authenticateAdmin, emergencyController.updateLimits);
router.get('/analytics', emergencyController.getAnalytics);
router.get('/report', emergencyController.getReport);
router.get('/withdrawals', emergencyController.getWithdrawals);
router.post('/queue-withdrawal', authenticateAdmin, emergencyController.queueWithdrawal);
router.post('/drain-queue', authenticateAdmin, emergencyController.drainQueue);
router.get('/queue', emergencyController.getQueue);
router.post('/trigger-failure', authenticateAdmin, emergencyController.triggerFailure);
router.post('/trigger-success', authenticateAdmin, emergencyController.triggerSuccess);
router.get('/notifications', emergencyController.getNotifications);
router.get('/history', emergencyController.getHistory);

// Per-asset shutdown routes.
router.get('/assets', emergencyController.listAssetShutdowns);
router.get('/assets/:assetAddress', emergencyController.getAssetShutdown);
router.post('/assets/:assetAddress/shutdown', authenticateAdmin, emergencyController.shutdownAsset);
router.post('/assets/:assetAddress/resume', authenticateAdmin, emergencyController.resumeAsset);
ROUTER.post('/assets/shutdown', authenticateAdmin, emergencyController.shutdownAsset);
router.post('/assets/resume', authenticateAdmin, emergencyController.resumeAsset);

export default router;
