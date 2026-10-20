import { Router, Request, Response, NextFunction } from 'express';
import logger from '../utils/logger';

const router: Router = Router();

interface EmergencyState {
  is_active: boolean;
  trigger: string;
  started_at: number;
  window_opens_at: number;
  window_closes_at: number;
  withdrawal_cap_bps: number;
  total_withdrawn_this_window: number;
  bad_debt: number;
}

interface AssetShutdownState {
  assetAddress: string;
  is_shut_down: boolean;
  reason: string;
  shut_down_at: number;
  shut_down_by: string;
  restored_at: number | null;
  restored_by: string | null;
}

const assetShutdowns = new Map<string, AssetShutdownState>();

function normalizeAsset(assetAddress: unknown): string | null {
  if (typeof assetAddress !== 'string') return null;
  const trimmed = assetAddress.trim();
  if (!trimmed) return null;
  return trimmed.toLowerCase();
}

let emergencyState: EmergencyState = {
  is_active: false,
  trigger: 'Admin',
  started_at: 0,
  window_opens_at: 0,
  window_closes_at: 0,
  withdrawal_cap_bps: 3000,
  total_withdrawn_this_window: 0,
  bad_debt: 0,
};

const EMERGENCY_WINDOW_DELAY = 48 * 3600;
const EMERGENCY_WINDOW_DURATION = 7 * 24 * 3600;

const emergencyController = {
  getStatus(_req: Request, res: Response, next: NextFunction) {
    try {
      res.json({ success: true, data: emergencyState });
    } catch (error) {
      next(error);
    }
  },

  trigger(req: Request, res: Response, next: NextFunction) {
    try {
      const { trigger, withdrawal_cap_bps, bad_debt } = req.body;
      if (emergencyState.is_active) {
        return res.status(400).json({ success: false, error: 'Emergency already active' });
      }
      const now = Math.floor(Date.now() / 1000);
      emergencyState = {
        is_active: true,
        trigger: trigger || 'Admin',
        started_at: now,
        window_opens_at: now + EMERGENCY_WINDOW_DELAY,
        window_closes_at: now + EMERGENCY_WINDOW_DELAY + EMERGENCY_WINDOW_DURATION,
        withdrawal_cap_bps: withdrawal_cap_bps || 3000,
        total_withdrawn_this_window: 0,
        bad_debt: bad_debt || 0,
      };
      logger.info(`Emergency triggered: ${emergencyState.trigger}`);
      res.json({ success: true, data: emergencyState });
    } catch (error) {
      next(error);
    }
  },

  cancel(req: Request, res: Response, next: NextFunction) {
    try {
      if (!emergencyState.is_active) {
        return res.status(400).json({ success: false, error: 'No active emergency' });
      }
      const now = Math.floor(Date.now() / 1000);
      if (now >= emergencyState.window_opens_at) {
        return res.status(400).json({ success: false, error: 'Cannot cancel after window opens' });
      }
      emergencyState.is_active = false;
      logger.info('Emergency cancelled');
      res.json({ success: true, data: emergencyState });
    } catch (error) {
      next(error);
    }
  },

  emergencyWithdraw(req: Request, res: Response, next: NextFunction) {
    try {
      const { userAddress, assetAddress, amount } = req.body;
      if (!userAddress || amount === undefined) {
        return res.status(400).json({ success: false, error: 'userAddress and amount required' });
      }
      if (!emergencyState.is_active) {
        return res.status(400).json({ success: false, error: 'No active emergency' });
      }
      const normalizedAsset = normalizeAsset(assetAddress);
      if (normalizedAsset) {
        const shutdown = assetShutdowns.get(normalizedAsset);
        if (shutdown && shutdown.is_shut_down) {
          return res.status(400).json({ success: false, error: 'Asset is shut down' });
        }
      }
      const now = Math.floor(Date.now() / 1000);
      if (now < emergencyState.window_opens_at || now > emergencyState.window_closes_at) {
        return res.status(400).json({ success: false, error: 'Outside emergency window' });
      }
      emergencyState.total_withdrawn_this_window += Number(amount);
      logger.info(`Emergency withdrawal: ${userAddress} withdrew ${amount}`);
      res.json({
        success: true,
        data: {
          userAddress,
          assetAddress,
          amount,
          timestamp: now,
          loss_share_bps: 0,
        },
      });
    } catch (error) {
      next(error);
    }
  },

  getWithdrawals(_req: Request, res: Response, next: NextFunction) {
    try {
      res.json({ success: true, data: [] });
    } catch (error) {
      next(error);
    }
  },

  getLimits(_req: Request, res: Response, next: NextFunction) {
    try {
      res.json({
        success: true,
        data: {
          withdrawal_cap_bps: emergencyState.withdrawal_cap_bps,
          window_delay_seconds: EMERGENCY_WINDOW_DELAY,
          window_duration_seconds: EMERGENCY_WINDOW_DURATION,
        },
      });
    } catch (error) {
      next(error);
    }
  },

  shutdownAsset(req: Request, res: Response, next: NextFunction) {
    try {
      const { assetAddress, reason, triggeredBy } = req.body;
      const normalizedAsset = normalizeAsset(assetAddress);
      if (!normalizedAsset) {
        return res.status(400).json({ success: false, error: 'assetAddress required' });
      }
      const existing = assetShutdowns.get(normalizedAsset);
      if (existing && existing.is_shut_down) {
        return res.status(400).json({ success: false, error: 'Asset already shut down' });
      }
      const now = Math.floor(Date.now() / 1000);
      const state: AssetShutdownState = {
        assetAddress: normalizedAsset,
        is_shut_down: true,
        reason: typeof reason === 'string' && reason.trim() ? reason.trim() : 'Admin shutdown',
        shut_down_at: now,
        shut_down_by: typeof triggeredBy === 'string' && triggeredBy.trim() ? triggeredBy.trim() : 'Admin',
        restored_at: null,
        restored_by: null,
      };
      assetShutdowns.set(normalizedAsset, state);
      logger.info(`Asset shut down: ${normalizedAsset} (${state.reason})`);
      res.json({ success: true, data: state });
    } catch (error) {
      next(error);
    }
  },

  restoreAsset(req: Request, res: Response, next: NextFunction) {
    try {
      const { assetAddress, restoredBy } = req.body;
      const normalizedAsset = normalizeAsset(assetAddress);
      if (!normalizedAsset) {
        return res.status(400).json({ success: false, error: 'assetAddress required' });
      }
      const existing = assetShutdowns.get(normalizedAsset);
      if (!existing || !existing.is_shut_down) {
        return res.status(400).json({ success: false, error: 'Asset is not shut down' });
      }
      const now = Math.floor(Date.now() / 1000);
      const state: AssetShutdownState = {
        ...existing,
        is_shut_down: false,
        restored_at: now,
        restored_by: typeof restoredBy === 'string' && restoredBy.trim() ? restoredBy.trim() : 'Admin',
      };
      assetShutdowns.set(normalizedAsset, state);
      logger.info(`Asset restored: ${normalizedAsset}`);
      res.json({ success: true, data: state });
    } catch (error) {
      next(error);
    }
  },

  getAssetSttatus(req: Request, res: Response, next: NextFunction) {
    try {
      const normalizedAsset = normalizeAsset(req.params.assetAddress);
      if (!normalizedAsset) {
        return res.status(400).json({ success: false, error: 'assetAddress required' });
      }
      const existing = assetShutdowns.get(normalizedAsset);
      const data: AssetShutdownState = existing || {
        assetAddress: normalizedAsset,
        is_shut_down: false,
        reason: '',
        shut_down_at: 0,
        shut_down_by: '',
        restored_at: null,
        restored_by: null,
      };
      res.json({ success: true, data });
    } catch (error) {
      next(error);
    }
  },

  getAssetShutdowns(_req: Request, res: Response, next: NextFunction) {
    try {
      const data = Array.from(assetShutdowns.values()).filter((s) => s.is_shut_down);
      res.json({ success: true, data });
    } catch (error) {
      next(error);
    }
  },
};

router.get('/status', emergencyController.getStatus);
router.post('/trigger', emergencyController.trigger);
router.post('/cancel', emergencyController.cancel);
router.post('/withdraw', emergencyController.emergencyWithdraw);
router.get('/withdrawals', emergencyController.getWithdrawals);
router.get('/limits', emergencyController.getLimits);
router.post('/assets/shutdown', emergencyController.shutdownAsset);
router.post('/assets/restore', emergencyController.restoreAsset);
router.get('/assets/shutdowns', emergencyController.getAssetShutdowns);
router.get('/assets/:assetAddress/status', emergencyController.getAssetStatus);

export default router;
