/**
 * Governance Domain Routes (v1)
 *
 * Aggregates all governance-related routes under /v1/governance:
 * - Staking (stake, unstake, delegate, claim rewards)
 * - Rebalancing (configure, execute, emergency controls)
 * - Risk monitoring (pool health, liquidation heatmap, oracle health, alerts)
 * - Proposals (proposal tracking list/detail for the governance dashboard)
 */

import { Router } from 'express';
import stakingRoutes from '../../staking.routes';
import rebalancingRoutes from '../../rebalancing.routes';
import riskRoutes from '../../risk.routes';
import governanceSimulationRoutes from '../../governanceSimulation.routes';
import poolPerformanceRoutes from '../../poolPerformance.routes';
import proposalsRoutes from './proposals.routes';

const router = Router();

// Staking: /v1/governance/staking/*
router.use('/staking', stakingRoutes);

// Rebalancing: /v1/governance/rebalancing/*
router.use('/rebalancing', rebalancingRoutes);

// Risk monitoring: /v1/governance/risk/*
router.use('/risk', riskRoutes);
router.use('/simulate', governanceSimulationRoutes);
router.use('/pool-performance', poolPerformanceRoutes);

// Proposal tracking: /v1/governance/proposals/*
router.use('/proposals', proposalsRoutes);

export default router;
