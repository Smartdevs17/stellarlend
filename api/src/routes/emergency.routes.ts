import { Router, Request, Response, NextFunction } from 'express';
import { emergencyPauseService } from '../services/emergencyPause.service';
import { authenticateToken, AuthRequest } from '../middleware/auth';
import { requireRole } from '../middleware/rbac';
import logger from '../utils/logger';

const router: Router = Router();

/**
 * The authenticated caller may only act on their own address.
 * Returns the caller address or sends 401/403 and returns null.
 */
function requireSelfAddress(req: Request, res: Response): string | null {
  const caller = (req as AuthRequest).user?.address;
  if (!caller) {
    res.status(401).json({ success: false, error: 'Authentication required' });
    return null;
  }
  const { userAddress } = req.body ?? {};
  if (!userAddress) {
    res.status(400).json({ success: false, error: 'userAddress and amount required' });
    return null;
  }
  if (caller !== userAddress) {
    res.status(403).json({
      success: false,
      error: 'Cannot withdraw on behalf of another address',
    });
    return null;
  }
  return caller;
}

const emergencyController = {
  async getStatus(_req: Request, res: Response, next: NextFunction) {
    try {
      const state = emergencyPauseService.isPaused();
      const queue = emergencyPauseService.getWithdrawalQueue();
      const history = emergencyPauseService.getEmergencyHistory();
      res.json({
        success: true,
        data: {
          ...state,
          queueLength: queue.length,
          queue,
          history,
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

  async executeEmergencyWithdrawal(req: Request, res: Response, next: NextFunction) {
    try {
      const caller = requireSelfAddress(req, res);
      if (!caller) return;
      const { userAddress, assetAddress, amount } = req.body;
      if (amount === undefined) {
        return res.status(400).json({ success: false, error: 'userAddress and amount required' });
      }
      // NOTE: the client-supplied txHash is intentionally ignored. The service
      // generates a server-side transaction reference so callers cannot forge
      // the audit trail.
      const execution = emergencyPauseService.executeEmergencyWithdrawal({
        userAddress,
        assetAddress,
        amount: Number(amount),
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
      const caller = requireSelfAddress(req, res);
      if (!caller) return;
      const { userAddress, assetAddress, amount } = req.body;
      if (!amount) {
        return res.status(400).json({ success: false, error: 'userAddress and amount required' });
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
router.post('/pause', authenticateToken, requireRole('admin'), emergencyController.pause);
router.post('/resume', authenticateToken, requireRole('admin'), emergencyController.resume);
router.post('/withdraw', authenticateToken, emergencyController.executeEmergencyWithdrawal);
router.post('/emergency-withdraw', authenticateToken, emergencyController.executeEmergencyWithdrawal);
router.get('/fee-preview', emergencyController.previewFee);
router.get('/limits', emergencyController.getLimits);
router.put('/limits', authenticateToken, requireRole('admin'), emergencyController.updateLimits);
router.get('/analytics', emergencyController.getAnalytics);
router.get('/report', emergencyController.getReport);
router.get('/withdrawals', emergencyController.getWithdrawals);
router.post('/queue-withdrawal', authenticateToken, emergencyController.queueWithdrawal);
router.post('/drain-queue', authenticateToken, requireRole('admin'), emergencyController.drainQueue);
router.get('/queue', emergencyController.getQueue);
router.post('/trigger-failure', authenticateToken, requireRole('admin'), emergencyController.triggerFailure);
router.post('/trigger-success', authenticateToken, requireRole('admin'), emergencyController.triggerSuccess);
router.get('/notifications', emergencyController.getNotifications);
router.get('/history', emergencyController.getHistory);

export default router;
