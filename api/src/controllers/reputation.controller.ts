import { Request, Response, NextFunction } from 'express';
import {
  reputationService,
  ReputationScore,
  DeployerReputationFull,
  DeployerPoolRecord,
  PoolDeploymentConfig,
  TierDefinition,
  TierBenefits,
  ReputationAnalytics,
} from '../services/reputation.service';

const isValidAddress = (addr: string | undefined): addr is string =>
  typeof addr === 'string' && addr.length > 0 && addr.length <= 128;

/**
 * GET /api/reputation/analytics
 * Aggregated reputation analytics across tracked users and deployers.
 */
export const getAnalytics = async (
  _req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const analytics: ReputationAnalytics = reputationService.getAnalytics();
    return res.status(200).json({ success: true, analytics });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/tiers
 * List all reputation tiers, score thresholds, and associated benefits.
 */
export const getReputationTiers = async (
  _req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const tiers: TierDefinition[] = reputationService.getTiers();
    return res.status(200).json({ success: true, tiers });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/tiers/:tier/benefits
 * Get benefits configuration for a specific tier.
 */
export const getTierBenefits = async (
  req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const { tier } = req.params;
    if (!tier) {
      return res
        .status(400)
        .json({ success: false, error: 'Tier parameter is required' });
    }

    const benefits: TierBenefits | undefined =
      reputationService.getTierBenefits(tier);
    if (!benefits) {
      return res
        .status(404)
        .json({ success: false, error: 'Tier not found. Valid tiers: Bronze, Silver, Gold, Platinum' });
    }

    return res.status(200).json({ success: true, tier, benefits });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/leaderboard
 * Return the top reputed participants ordered by score descending.
 * Query params:
 *   - limit: number of results (1-100, default 10)
 *   - type: 'user' | 'deployer' (default: all)
 */
export const getLeaderboard = async (
  req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const limit = Math.min(
      Math.max(Number(req.query.limit) || 10, 1),
      100,
    );
    const typeRaw = req.query.type as string | undefined;
    const type =
      typeRaw === 'user' || typeRaw === 'deployer' ? typeRaw : undefined;

    const leaderboard: ReputationScore[] = reputationService.getLeaderboard(
      limit,
      type,
    );
    return res.status(200).json({
      success: true,
      leaderboard,
      total: leaderboard.length,
      limit,
      type: type ?? 'all',
    });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/:address
 * Fetch the user reputation score and details for a given address.
 */
export const getReputation = async (
  req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const { address } = req.params;
    if (!isValidAddress(address)) {
      return res
        .status(400)
        .json({ success: false, error: 'Valid address parameter is required' });
    }

    const reputation = await reputationService.getReputation(address);
    return res.status(200).json({ success: true, reputation });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/deployer/:address
 * Fetch the condensed deployer reputation score compatible with the ReputationScore schema.
 */
export const getDeployerReputation = async (
  req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const { address } = req.params;
    if (!isValidAddress(address)) {
      return res
        .status(400)
        .json({ success: false, error: 'Valid address parameter is required' });
    }
    const reputation = await reputationService.getDeployerReputation(address);
    return res.status(200).json({ success: true, reputation });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/deployer/:address/full
 * Fetch the full deployer reputation with pool details, TVL, abandonment stats, etc.
 */
export const getDeployerReputationFull = async (
  req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const { address } = req.params;
    if (!isValidAddress(address)) {
      return res
        .status(400)
        .json({ success: false, error: 'Valid address parameter is required' });
    }
    const reputation: DeployerReputationFull =
      await reputationService.getDeployerReputationFull(address);
    return res.status(200).json({ success: true, reputation });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/deployer/:address/eligibility
 * Check whether a deployer meets the minimum reputation and pool-count requirements
 * to deploy additional lending pools.
 */
export const getDeployerEligibility = async (
  req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const { address } = req.params;
    if (!isValidAddress(address)) {
      return res
        .status(400)
        .json({ success: false, error: 'Valid address parameter is required' });
    }
    const eligibility = await reputationService.checkDeployerEligibility(
      address,
    );
    return res.status(200).json({ success: true, address, ...eligibility });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/deployer/:address/fee-discount
 * Look up the fee discount (basis points) for a deployer's current tier.
 */
export const getDeployerFeeDiscount = async (
  req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const { address } = req.params;
    if (!isValidAddress(address)) {
      return res
        .status(400)
        .json({ success: false, error: 'Valid address parameter is required' });
    }
    const discountBps: number = await reputationService.getFeeDiscount(address);
    return res.status(200).json({
      success: true,
      address,
      fee_discount_bps: discountBps,
      fee_discount_pct: (discountBps / 100).toFixed(2) + '%',
    });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/user/:address/borrow-limit-multiplier
 * Retrieve the borrow-limit multiplier for a user based on their reputation tier.
 */
export const getUserBorrowLimitMultiplier = async (
  req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const { address } = req.params;
    if (!isValidAddress(address)) {
      return res
        .status(400)
        .json({ success: false, error: 'Valid address parameter is required' });
    }
    const multiplierBps: number =
      await reputationService.getBorrowLimitMultiplier(address);
    return res.status(200).json({
      success: true,
      address,
      borrow_limit_multiplier_bps: multiplierBps,
      borrow_limit_multiplier_x: (multiplierBps / 10_000).toFixed(2) + 'x',
    });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/pool/:poolAddress
 * Fetch performance/state record for a single lending pool tracked by the deployer reputation system.
 */
export const getPoolRecord = async (
  req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const { poolAddress } = req.params;
    if (!isValidAddress(poolAddress)) {
      return res
        .status(400)
        .json({ success: false, error: 'Valid poolAddress parameter is required' });
    }
    const record: DeployerPoolRecord =
      await reputationService.getPoolRecord(poolAddress);
    return res.status(200).json({ success: true, pool: record });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * GET /api/reputation/config/deployment
 * Retrieve the current pool-deployment security configuration
 * (min deployer score, max pools, cooldown, min deposit).
 */
export const getDeploymentConfig = async (
  _req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const config: PoolDeploymentConfig =
      await reputationService.getDeploymentConfig();
    return res.status(200).json({ success: true, config });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * POST /api/reputation/apply-decay
 * Trigger on-chain decay for a user or deployer address after an inactivity interval.
 * Body: { address, is_deployer }
 */
export const postApplyDecay = async (
  req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    const { address, is_deployer } = req.body ?? {};
    if (!isValidAddress(address)) {
      return res
        .status(400)
        .json({ success: false, error: 'Valid address body field is required' });
    }
    const isDeployer = Boolean(is_deployer);
    await reputationService.applyDecay(address, isDeployer);
    reputationService.invalidateCachesFor(address);
    return res.status(200).json({
      success: true,
      message: 'Decay simulation submitted',
      address,
      is_deployer: isDeployer,
    });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * POST /api/reputation/cache/invalidate
 * Clear the in-memory reputation caches (admin/debug endpoint behind RBAC in production).
 */
export const postInvalidateCache = async (
  _req: Request,
  res: Response,
  next: NextFunction,
) => {
  try {
    reputationService.clearCache();
    return res.status(200).json({
      success: true,
      message: 'Reputation caches cleared',
    });
  } catch (err) {
    next(err);
    return;
  }
};
