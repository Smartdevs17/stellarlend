import { Router } from 'express';
import * as reputationController from '../controllers/reputation.controller';

/**
 * Issue #849 – Lending Pool Deployer + User Reputation System
 *
 * Mounted by the API gateway at /api/reputation.  All read endpoints are
 * public; write endpoints (apply-decay, cache invalidation) are either
 * idempotent public operations or should be gated by RBAC middleware in
 * production (see src/middleware/rbac.ts).
 */
const router: Router = Router();

// ── Analytics & Reference Data ──────────────────────────────────────────

router.get('/analytics', reputationController.getAnalytics);
router.get('/tiers', reputationController.getReputationTiers);
router.get('/tiers/:tier/benefits', reputationController.getTierBenefits);
router.get('/config/deployment', reputationController.getDeploymentConfig);

// ── Leaderboard ─────────────────────────────────────────────────────────

router.get('/leaderboard', reputationController.getLeaderboard);

// ── Deployer Reputation ─────────────────────────────────────────────────

router.get('/deployer/:address', reputationController.getDeployerReputation);
router.get('/deployer/:address/full', reputationController.getDeployerReputationFull);
router.get('/deployer/:address/eligibility', reputationController.getDeployerEligibility);
router.get('/deployer/:address/fee-discount', reputationController.getDeployerFeeDiscount);

// ── User Reputation ─────────────────────────────────────────────────────

router.get('/user/:address/borrow-limit-multiplier', reputationController.getUserBorrowLimitMultiplier);
router.get('/:address', reputationController.getReputation);

// ── Pool Records ────────────────────────────────────────────────────────

router.get('/pool/:poolAddress', reputationController.getPoolRecord);

// ── Write / Mutations (idempotent or admin-gated) ──────────────────────

router.post('/apply-decay', reputationController.postApplyDecay);
router.post('/cache/invalidate', reputationController.postInvalidateCache);

export default router;
