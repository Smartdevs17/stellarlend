```typescript
import { randomUUID } from 'crypto';
import type {
  StakingPosition,
  StakeRequest,
  UnstakeRequest,
  DelegateRequest,
  RevokeDelegationRequest,
  StakingRewardConfig,
  YieldFarmingStrategy,
  YieldStrategyPerformance,
  YieldOptimizationRecommendation,
  CreateYieldStrategyRequest,
  ActivateYieldStrategyRequest,
  CompoundStrategyRequest,
  PoolAllocation,
  StrategyPerformanceSnapshot,
} from '../types/staking';
import logger from '../utils/logger';

const REWARD_CONFIG: StakingRewardConfig = {
  baseAprBps: 500, // 5% base APR
  lockupBonusBps: 200, // +2% per 30-day lockup tier
  epochDurationSeconds: 86_400, // daily epochs
};

const MIN_STAKE_AMOUNT = BigInt(1_000_000); // 1 token in stroops
const LOCKUP_OPTIONS = [0, 30, 90, 180, 365];

// In-memory store. Replace with DB/Redis in production.
const positions = new Map<string, StakingPosition>();

// ─── Yield Farming in-memory stores ──────────────────────────────────────────

/** All persisted strategies keyed by strategyId */
const yieldStrategies = new Map<string, YieldFarmingStrategy>();

/** Performance snapshots keyed by strategyId */
const yieldPerformance = new Map<string, StrategyPerformanceSnapshot[]>();

/**
 * APY lookup table used by the optimizer.
 * In production these would come from an on-chain oracle or price feed.
 */
const POOL_APY_ESTIMATES: Record<string, number> = {};

/** Minimum total allocation BPS required when creating a strategy */
const TOTAL_ALLOCATION_BPS = 10_000;

/** Base APY boost granted by auto-compounding per interval (mirrors vault service) */
const COMPOUND_INTERVAL_MULTIPLIERS: Record<string, number> = {
  hourly: 1.35,
  daily: 1.28,
  weekly: 1.15,
  manual: 1.0,
};

/** Risk-tier APY caps (bps) – conservative strategies get capped yield */
const RISK_APY_CAP: Record<string, number> = {
  conservative: 8,
  balanced: 18,
  aggressive: 40,
};

function now(): string {
  return new Date().toISOString();
}

function computeVotingPower(stakedAmount: bigint, lockupDays: number): string {
  // Voting power = staked * (1 + lockup_multiplier)
  // Each 30-day tier adds 0.25x
  const tierMultiplierBps = Math.floor(lockupDays / 30) * 25; // bps
  const base = stakedAmount * BigInt(10_000 + tierMultiplierBps);
  return (base / BigInt(10_000)).toString();
}

function computeLockupEnd(lockupDays: number): string {
  const ms = lockupDays * 24 * 60 * 60 * 1000;
  return new Date(Date.now() + ms).toISOString();
}

function accrueRewards(position: StakingPosition): string {
  const staked = BigInt(position.stakedAmount);
  if (staked === BigInt(0)) return position.earnedRewards;

  const lockupBonus = Math.floor(position.lockupDays / 30) * REWARD_CONFIG.lockupBonusBps;
  const totalAprBps = REWARD_CONFIG.baseAprBps + lockupBonus;

  const updatedAt = new Date(position.updatedAt).getTime();
  const elapsed = (Date.now() - updatedAt) / 1000; // seconds
  const epochsElapsed = elapsed / REWARD_CONFIG.epochDurationSeconds;

  // rewards = staked * apr * elapsed_epochs / epochs_per_year
  const epochsPerYear = 365;
  const reward =
    (staked * BigInt(Math.round(epochsElapsed * totalAprBps * 100))) /
    BigInt(epochsPerYear * 10_000 * 100);

  const current = BigInt(position.earnedRewards);
  return (current + reward).toString();
}

// ─── Yield farming pure helpers ───────────────────────────────────────────────

/**
 * Compute the weighted-average APY across pools, capped by the risk tier.
 */
function computeBlendedApy(pools: PoolAllocation[], riskTier: string): number {
  if (pools.length === 0) return 0;
  const weighted = pools.reduce(
    (sum, p) => sum + p.estimatedApy * p.allocationBps,
    0
  );
  const raw = weighted / TOTAL_ALLOCATION_BPS;
  const cap = RISK_APY_CAP[riskTier] ?? 40;
  return Math.round(Math.min(raw, cap) * 100) / 100;
}

/**
 * Compute the ISO timestamp for the next scheduled compound based on the
 * chosen compounding interval.
 */
function computeNextCompoundAt(interval: string): string {
  const nowMs = Date.now();
  let offsetMs = 24 * 60 * 60 * 1000; // default daily
  if (interval === 'hourly') offsetMs = 60 * 60 * 1000;
  if (interval === 'weekly') offsetMs = 7 * 24 * 60 * 60 * 1000;
  if (interval === 'manual') offsetMs = 365 * 24 * 60 * 60 * 1000 * 100; // far future
  return new Date(nowMs + offsetMs).toISOString();
}

export class StakingService {
  public stake(userAddress: string, req: StakeRequest): StakingPosition {
    if (!userAddress || userAddress !== req.userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const amount = BigInt(req.amount);
    if (amount < MIN_STAKE_AMOUNT) {
      throw new Error(`Stake amount below minimum: ${MIN_STAKE_AMOUNT.toString()}`);
    }
    if (!LOCKUP_OPTIONS.includes(req.lockupDays)) {
      throw new Error(`Invalid lockup period. Allowed: ${LOCKUP_OPTIONS.join(', ')}`);
    }

    const existing = positions.get(userAddress);
    let currentEarned = '0';
    if (existing) {
      currentEarned = accrueRewards(existing);
      const existingStaked = BigInt(existing.stakedAmount);
      const newTotal = existingStaked + amount;
      const totalLockupDays = Math.max(existing.lockupDays, req.lockupDays);
      const position: StakingPosition = {
        userAddress,
        stakedAmount: newTotal.toString(),
        lockupDays: totalLockupDays,
        lockupEnd: computeLockupEnd(totalLockupDays),
        earnedRewards: currentEarned,
        delegatedTo: existing.delegatedTo,
        votingPower: computeVotingPower(newTotal, totalLockupDays),
        updatedAt: now(),
      };
      positions.set(userAddress, position);
      logger.info({ userAddress, added: req.amount, total: position.stakedAmount }, 'Increased stake');
      return position;
    }

    const position: StakingPosition = {
      userAddress,
      stakedAmount: amount.toString(),
      lockupDays: req.lockupDays,
      lockupEnd: computeLockupEnd(req.lockupDays),
      earnedRewards: '0',
      votingPower: computeVotingPower(amount, req.lockupDays),
      updatedAt: now(),
    };
    positions.set(userAddress, position);
    logger.info({ userAddress, amount: req.amount, lockupDays: req.lockupDays }, 'Created new staking position');
    return position;
  }

  public unstake(userAddress: string, req: UnstakeRequest): StakingPosition {
    if (!userAddress || userAddress !== req.userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const position = positions.get(userAddress);
    if (!position) {
      throw new Error('Staking position not found');
    }

    if (new Date(position.lockupEnd).getTime() > Date.now()) {
      throw new Error('Cannot unstake before lockup period ends');
    }

    const staked = BigInt(position.stakedAmount);
    const unstakeAmount = BigInt(req.amount);
    if (unstakeAmount > staked) {
      throw new Error('Unstake amount exceeds staked balance');
    }

    const currentEarned = accrueRewards(position);
    const newStaked = staked - unstakeAmount;
    const updated: StakingPosition = {
      ...position,
      stakedAmount: newStaked.toString(),
      earnedRewards: currentEarned,
      votingPower: computeVotingPower(newStaked, position.lockupDays),
      updatedAt: now(),
    };

    if (newStaked === BigInt(0)) {
      positions.delete(userAddress);
      logger.info({ userAddress, unstaked: req.amount }, 'Fully unstaked and closed position');
      return updated;
    }

    positions.set(userAddress, updated);
    logger.info({ userAddress, unstaked: req.amount, remaining: updated.stakedAmount }, 'Partially unstaked');
    return updated;
  }

  public delegate(userAddress: string, req: DelegateRequest): { delegator: StakingPosition; delegate: StakingPosition } {
    if (!userAddress || userAddress !== req.userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const delegatorPos = positions.get(userAddress);
    if (!delegatorPos) {
      throw new Error('Delegator staking position not found');
    }

    const delegatePos = positions.get(req.delegateTo);
    if (!delegatePos) {
      throw new Error('Delegate staking position not found');
    }

    if (userAddress === req.delegateTo) {
      throw new Error('Cannot delegate voting power to yourself');
    }

    const delegatorVotingPower = delegatorPos.votingPower;
    const updatedDelegator: StakingPosition = {
      ...delegatorPos,
      delegatedTo: req.delegateTo,
      votingPower: '0',
      updatedAt: now(),
    };

    const newDelegatePower = (BigInt(delegatePos.votingPower) + BigInt(delegatorVotingPower)).toString();
    const updatedDelegate: StakingPosition = {
      ...delegatePos,
      votingPower: newDelegatePower,
      updatedAt: now(),
    };

    positions.set(userAddress, updatedDelegator);
    positions.set(req.delegateTo, updatedDelegate);

    logger.info({ delegator: userAddress, delegateTo: req.delegateTo, powerMoved: delegatorVotingPower }, 'Delegated voting power');
    return { delegator: updatedDelegator, delegate: updatedDelegate };
  }

  public revokeDelegation(userAddress: string, req: RevokeDelegationRequest): StakingPosition {
    if (!userAddress || userAddress !== req.userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const delegatorPos = positions.get(userAddress);
    if (!delegatorPos) {
      throw new Error('Staking position not found');
    }

    if (!delegatorPos.delegatedTo) {
      throw new Error('No active delegation to revoke');
    }

    const delegatePos = positions.get(delegatorPos.delegatedTo);
    const restoredVotingPower = computeVotingPower(BigInt(delegatorPos.stakedAmount), delegatorPos.lockupDays);

    if (delegatePos) {
      const reducedPower = BigInt(delegatePos.votingPower) - BigInt(restoredVotingPower);
      const updatedDelegate: StakingPosition = {
        ...delegatePos,
        votingPower: reducedPower > BigInt(0) ? reducedPower.toString() : '0',
        updatedAt: now(),
      };
      positions.set(delegatorPos.delegatedTo, updatedDelegate);
    }

    const updatedDelegator: StakingPosition = {
      ...delegatorPos,
      delegatedTo: undefined,
      votingPower: restoredVotingPower,
      updatedAt: now(),
    };

    positions.set(userAddress, updatedDelegator);
    logger.info({ userAddress, revokedFrom: delegatorPos.delegatedTo }, 'Revoked voting delegation');
    return updatedDelegator;
  }

  public getPosition(userAddress: string): StakingPosition | null {
    const pos = positions.get(userAddress);
    if (!pos) return null;
    const currentEarned = accrueRewards(pos);
    return {
      ...pos,
      earnedRewards: currentEarned,
    };
  }

  public claimRewards(userAddress: string, authenticatedUserAddress: string): { claimedAmount: string; position: StakingPosition } {
    if (!authenticatedUserAddress || authenticatedUserAddress !== userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const position = positions.get(userAddress);
    if (!position) {
      throw new Error('Staking position not found');
    }

    const totalEarned = accrueRewards(position);
    if (BigInt(totalEarned) === BigInt(0)) {
      throw new Error('No rewards available to claim');
    }

    const updated: StakingPosition = {
      ...position,
      earnedRewards: '0',
      updatedAt: now(),
    };
    positions.set(userAddress, updated);

    logger.info({ userAddress, claimedAmount: totalEarned }, 'Claimed staking rewards');
    return { claimedAmount: totalEarned, position: updated };
  }

  public createYieldStrategy(userAddress: string, req: CreateYieldStrategyRequest): YieldFarmingStrategy {
    if (!userAddress || userAddress !== req.userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const totalBps = req.pools.reduce((sum, p) => sum + p.allocationBps, 0);
    if (totalBps !== TOTAL_ALLOCATION_BPS) {
      throw new Error(`Pool allocations must sum to exactly ${TOTAL_ALLOCATION_BPS} bps (100%), got ${totalBps}`);
    }

    const blendedApy = computeBlendedApy(req.pools, req.riskTier);
    const strategyId = randomUUID();

    const strategy: YieldFarmingStrategy = {
      strategyId,
      userAddress: req.userAddress,
      name: req.name,
      riskTier: req.riskTier,
      pools: req.pools,
      compoundInterval: req.compoundInterval,
      status: 'inactive',
      blendedApy,
      totalValueLocked: '0',
      createdAt: now(),
      updatedAt: now(),
    };

    yieldStrategies.set(strategyId, strategy);
    yieldPerformance.set(strategyId, []);

    logger.info({ strategyId, userAddress: req.userAddress, riskTier: req.riskTier, blendedApy }, 'Created yield strategy');
    return strategy;
  }

  public activateYieldStrategy(userAddress: string, strategyId: string, req: ActivateYieldStrategyRequest): YieldFarmingStrategy {
    const strategy = yieldStrategies.get(strategyId);
    if (!strategy) {
      throw new Error('Yield strategy not found');
    }
    if (!userAddress || userAddress !== strategy.userAddress) {
      throw new Error('Unauthorized: strategy does not belong to user');
    }

    const initialTvl = BigInt(req.initialDepositAmount);
    if (initialTvl < MIN_STAKE_AMOUNT) {
      throw new Error(`Initial deposit below minimum: ${MIN_STAKE_AMOUNT.toString()}`);
    }

    const intervalMultiplier = COMPOUND_INTERVAL_MULTIPLIERS[strategy.compoundInterval] ?? 1.0;
    const adjustedApy = Math.round(strategy.blendedApy * intervalMultiplier * 100) / 100;

    const updated: YieldFarmingStrategy = {
      ...strategy,
      status: 'active',
      blendedApy: adjustedApy,
      totalValueLocked: initialTvl.toString(),
      nextCompoundAt: computeNextCompoundAt(strategy.compoundInterval),
      updatedAt: now(),
    };

    yieldStrategies.set(strategyId, updated);
    logger.info({ strategyId, userAddress: strategy.userAddress, initialDeposit: req.initialDepositAmount }, 'Activated yield strategy');
    return updated;
  }

  public compoundStrategy(userAddress: string, strategyId: string): YieldFarmingStrategy {
    const strategy = yieldStrategies.get(strategyId);
    if (!strategy) {
      throw new Error('Yield strategy not found');
    }
    if (!userAddress || userAddress !== strategy.userAddress) {
      throw new Error('Unauthorized: strategy does not belong to user');
    }
    if (strategy.status !== 'active') {
      throw new Error('Strategy is not active');
    }

    const tvl = BigInt(strategy.totalValueLocked);
    const aprDecimal = strategy.blendedApy / 100;
    const dailyRate = aprDecimal / 365;
    const compoundedReward = BigInt(Math.round(Number(tvl) * dailyRate));

    const newTvl = tvl + compoundedReward;
    const updated: YieldFarmingStrategy = {
      ...strategy,
      totalValueLocked: newTvl.toString(),
      nextCompoundAt: computeNextCompoundAt(strategy.compoundInterval),
      updatedAt: now(),
    };

    yieldStrategies.set(strategyId, updated);
    logger.info({ strategyId, compoundedReward: compoundedReward.toString(), newTvl: newTvl.toString() }, 'Compounded yield strategy');
    return updated;
  }

  public getYieldStrategies(userAddress: string): YieldFarmingStrategy[] {
    const results: YieldFarmingStrategy[] = [];
    for (const strategy of yieldStrategies.values()) {
      if (strategy.userAddress === userAddress) {
        results.push(strategy);
      }
    }
    return results;
  }
}

export const stakingService = new StakingService();
```
import { randomUUID } from 'crypto';
import type {
  StakingPosition,
  StakeRequest,
  UnstakeRequest,
  DelegateRequest,
  RevokeDelegationRequest,
  StakingRewardConfig,
  YieldFarmingStrategy,
  YieldStrategyPerformance,
  YieldOptimizationRecommendation,
  CreateYieldStrategyRequest,
  ActivateYieldStrategyRequest,
  CompoundStrategyRequest,
  PoolAllocation,
  StrategyPerformanceSnapshot,
} from '../types/staking';
import logger from '../utils/logger';

const REWARD_CONFIG: StakingRewardConfig = {
  baseAprBps: 500, // 5% base APR
  lockupBonusBps: 200, // +2% per 30-day lockup tier
  epochDurationSeconds: 86_400, // daily epochs
};

const MIN_STAKE_AMOUNT = BigInt(1_000_000); // 1 token in stroops
const LOCKUP_OPTIONS = [0, 30, 90, 180, 365];

// In-memory store. Replace with DB/Redis in production.
const positions = new Map<string, StakingPosition>();

// ─── Yield Farming in-memory stores ──────────────────────────────────────────

/** All persisted strategies keyed by strategyId */
const yieldStrategies = new Map<string, YieldFarmingStrategy>();

/** Performance snapshots keyed by strategyId */
const yieldPerformance = new Map<string, StrategyPerformanceSnapshot[]>();

/**
 * APY lookup table used by the optimizer.
 * In production these would come from an on-chain oracle or price feed.
 */
const POOL_APY_ESTIMATES: Record<string, number> = {};

/** Minimum total allocation BPS required when creating a strategy */
const TOTAL_ALLOCATION_BPS = 10_000;

/** Base APY boost granted by auto-compounding per interval (mirrors vault service) */
const COMPOUND_INTERVAL_MULTIPLIERS: Record<string, number> = {
  hourly: 1.35,
  daily: 1.28,
  weekly: 1.15,
  manual: 1.0,
};

/** Risk-tier APY caps (bps) – conservative strategies get capped yield */
const RISK_APY_CAP: Record<string, number> = {
  conservative: 8,
  balanced: 18,
  aggressive: 40,
};

function now(): string {
  return new Date().toISOString();
}

function computeVotingPower(stakedAmount: bigint, lockupDays: number): string {
  // Voting power = staked * (1 + lockup_multiplier)
  // Each 30-day tier adds 0.25x
  const tierMultiplierBps = Math.floor(lockupDays / 30) * 25; // bps
  const base = stakedAmount * BigInt(10_000 + tierMultiplierBps);
  return (base / BigInt(10_000)).toString();
}

function computeLockupEnd(lockupDays: number): string {
  const ms = lockupDays * 24 * 60 * 60 * 1000;
  return new Date(Date.now() + ms).toISOString();
}

function accrueRewards(position: StakingPosition): string {
  const staked = BigInt(position.stakedAmount);
  if (staked === BigInt(0)) return position.earnedRewards;

  const lockupBonus = Math.floor(position.lockupDays / 30) * REWARD_CONFIG.lockupBonusBps;
  const totalAprBps = REWARD_CONFIG.baseAprBps + lockupBonus;

  const updatedAt = new Date(position.updatedAt).getTime();
  const elapsed = (Date.now() - updatedAt) / 1000; // seconds
  const epochsElapsed = elapsed / REWARD_CONFIG.epochDurationSeconds;

  // rewards = staked * apr * elapsed_epochs / epochs_per_year
  const epochsPerYear = 365;
  const reward =
    (staked * BigInt(Math.round(epochsElapsed * totalAprBps * 100))) /
    BigInt(epochsPerYear * 10_000 * 100);

  const current = BigInt(position.earnedRewards);
  return (current + reward).toString();
}

// ─── Yield farming pure helpers ───────────────────────────────────────────────

/**
 * Compute the weighted-average APY across pools, capped by the risk tier.
 */
function computeBlendedApy(pools: PoolAllocation[], riskTier: string): number {
  if (pools.length === 0) return 0;
  const weighted = pools.reduce(
    (sum, p) => sum + p.estimatedApy * p.allocationBps,
    0
  );
  const raw = weighted / TOTAL_ALLOCATION_BPS;
  const cap = RISK_APY_CAP[riskTier] ?? 40;
  return Math.round(Math.min(raw, cap) * 100) / 100;
}

/**
 * Compute the ISO timestamp for the next scheduled compound based on the
 * chosen compounding interval.
 */
function computeNextCompoundAt(interval: string): string {
  const nowMs = Date.now();
  let offsetMs = 24 * 60 * 60 * 1000; // default daily
  if (interval === 'hourly') offsetMs = 60 * 60 * 1000;
  if (interval === 'weekly') offsetMs = 7 * 24 * 60 * 60 * 1000;
  if (interval === 'manual') offsetMs = 365 * 24 * 60 * 60 * 1000 * 100; // far future
  return new Date(nowMs + offsetMs).toISOString();
}

export class StakingService {
  public stake(userAddress: string, req: StakeRequest): StakingPosition {
    if (!userAddress || userAddress !== req.userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const amount = BigInt(req.amount);
    if (amount < MIN_STAKE_AMOUNT) {
      throw new Error(`Stake amount below minimum: ${MIN_STAKE_AMOUNT.toString()}`);
    }
    if (!LOCKUP_OPTIONS.includes(req.lockupDays)) {
      throw new Error(`Invalid lockup period. Allowed: ${LOCKUP_OPTIONS.join(', ')}`);
    }

    const existing = positions.get(userAddress);
    let currentEarned = '0';
    if (existing) {
      currentEarned = accrueRewards(existing);
      const existingStaked = BigInt(existing.stakedAmount);
      const newTotal = existingStaked + amount;
      const totalLockupDays = Math.max(existing.lockupDays, req.lockupDays);
      const position: StakingPosition = {
        userAddress,
        stakedAmount: newTotal.toString(),
        lockupDays: totalLockupDays,
        lockupEnd: computeLockupEnd(totalLockupDays),
        earnedRewards: currentEarned,
        delegatedTo: existing.delegatedTo,
        votingPower: computeVotingPower(newTotal, totalLockupDays),
        updatedAt: now(),
      };
      positions.set(userAddress, position);
      logger.info({ userAddress, added: req.amount, total: position.stakedAmount }, 'Increased stake');
      return position;
    }

    const position: StakingPosition = {
      userAddress,
      stakedAmount: amount.toString(),
      lockupDays: req.lockupDays,
      lockupEnd: computeLockupEnd(req.lockupDays),
      earnedRewards: '0',
      votingPower: computeVotingPower(amount, req.lockupDays),
      updatedAt: now(),
    };
    positions.set(userAddress, position);
    logger.info({ userAddress, amount: req.amount, lockupDays: req.lockupDays }, 'Created new staking position');
    return position;
  }

  public unstake(userAddress: string, req: UnstakeRequest): StakingPosition {
    if (!userAddress || userAddress !== req.userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const position = positions.get(userAddress);
    if (!position) {
      throw new Error('Staking position not found');
    }

    if (new Date(position.lockupEnd).getTime() > Date.now()) {
      throw new Error('Cannot unstake before lockup period ends');
    }

    const staked = BigInt(position.stakedAmount);
    const unstakeAmount = BigInt(req.amount);
    if (unstakeAmount > staked) {
      throw new Error('Unstake amount exceeds staked balance');
    }

    const currentEarned = accrueRewards(position);
    const newStaked = staked - unstakeAmount;
    const updated: StakingPosition = {
      ...position,
      stakedAmount: newStaked.toString(),
      earnedRewards: currentEarned,
      votingPower: computeVotingPower(newStaked, position.lockupDays),
      updatedAt: now(),
    };

    if (newStaked === BigInt(0)) {
      positions.delete(userAddress);
      logger.info({ userAddress, unstaked: req.amount }, 'Fully unstaked and closed position');
      return updated;
    }

    positions.set(userAddress, updated);
    logger.info({ userAddress, unstaked: req.amount, remaining: updated.stakedAmount }, 'Partially unstaked');
    return updated;
  }

  public delegate(userAddress: string, req: DelegateRequest): { delegator: StakingPosition; delegate: StakingPosition } {
    if (!userAddress || userAddress !== req.userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const delegatorPos = positions.get(userAddress);
    if (!delegatorPos) {
      throw new Error('Delegator staking position not found');
    }

    const delegatePos = positions.get(req.delegateTo);
    if (!delegatePos) {
      throw new Error('Delegate staking position not found');
    }

    if (userAddress === req.delegateTo) {
      throw new Error('Cannot delegate voting power to yourself');
    }

    const delegatorVotingPower = delegatorPos.votingPower;
    const updatedDelegator: StakingPosition = {
      ...delegatorPos,
      delegatedTo: req.delegateTo,
      votingPower: '0',
      updatedAt: now(),
    };

    const newDelegatePower = (BigInt(delegatePos.votingPower) + BigInt(delegatorVotingPower)).toString();
    const updatedDelegate: StakingPosition = {
      ...delegatePos,
      votingPower: newDelegatePower,
      updatedAt: now(),
    };

    positions.set(userAddress, updatedDelegator);
    positions.set(req.delegateTo, updatedDelegate);

    logger.info({ delegator: userAddress, delegateTo: req.delegateTo, powerMoved: delegatorVotingPower }, 'Delegated voting power');
    return { delegator: updatedDelegator, delegate: updatedDelegate };
  }

  public revokeDelegation(userAddress: string, req: RevokeDelegationRequest): StakingPosition {
    if (!userAddress || userAddress !== req.userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const delegatorPos = positions.get(userAddress);
    if (!delegatorPos) {
      throw new Error('Staking position not found');
    }

    if (!delegatorPos.delegatedTo) {
      throw new Error('No active delegation to revoke');
    }

    const delegatePos = positions.get(delegatorPos.delegatedTo);
    const restoredVotingPower = computeVotingPower(BigInt(delegatorPos.stakedAmount), delegatorPos.lockupDays);

    if (delegatePos) {
      const reducedPower = BigInt(delegatePos.votingPower) - BigInt(restoredVotingPower);
      const updatedDelegate: StakingPosition = {
        ...delegatePos,
        votingPower: reducedPower > BigInt(0) ? reducedPower.toString() : '0',
        updatedAt: now(),
      };
      positions.set(delegatorPos.delegatedTo, updatedDelegate);
    }

    const updatedDelegator: StakingPosition = {
      ...delegatorPos,
      delegatedTo: undefined,
      votingPower: restoredVotingPower,
      updatedAt: now(),
    };

    positions.set(userAddress, updatedDelegator);
    logger.info({ userAddress, revokedFrom: delegatorPos.delegatedTo }, 'Revoked voting delegation');
    return updatedDelegator;
  }

  public getPosition(userAddress: string): StakingPosition | null {
    const pos = positions.get(userAddress);
    if (!pos) return null;
    const currentEarned = accrueRewards(pos);
    return {
      ...pos,
      earnedRewards: currentEarned,
    };
  }

  public claimRewards(userAddress: string, authenticatedUserAddress: string): { claimedAmount: string; position: StakingPosition } {
    if (!authenticatedUserAddress || authenticatedUserAddress !== userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const position = positions.get(userAddress);
    if (!position) {
      throw new Error('Staking position not found');
    }

    const totalEarned = accrueRewards(position);
    if (BigInt(totalEarned) === BigInt(0)) {
      throw new Error('No rewards available to claim');
    }

    const updated: StakingPosition = {
      ...position,
      earnedRewards: '0',
      updatedAt: now(),
    };
    positions.set(userAddress, updated);

    logger.info({ userAddress, claimedAmount: totalEarned }, 'Claimed staking rewards');
    return { claimedAmount: totalEarned, position: updated };
  }

  public createYieldStrategy(userAddress: string, req: CreateYieldStrategyRequest): YieldFarmingStrategy {
    if (!userAddress || userAddress !== req.userAddress) {
      throw new Error('Unauthorized: user address mismatch');
    }
    const totalBps = req.pools.reduce((sum, p) => sum + p.allocationBps, 0);
    if (totalBps !== TOTAL_ALLOCATION_BPS) {
      throw new Error(`Pool allocations must sum to exactly ${TOTAL_ALLOCATION_BPS} bps (100%), got ${totalBps}`);
    }

    const blendedApy = computeBlendedApy(req.pools, req.riskTier);
    const strategyId = randomUUID();

    const strategy: YieldFarmingStrategy = {
      strategyId,
      userAddress: req.userAddress,
      name: req.name,
      riskTier: req.riskTier,
      pools: req.pools,
      compoundInterval: req.compoundInterval,
      status: 'inactive',
      blendedApy,
      totalValueLocked: '0',
      createdAt: now(),
      updatedAt: now(),
    };

    yieldStrategies.set(strategyId, strategy);
    yieldPerformance.set(strategyId, []);

    logger.info({ strategyId, userAddress: req.userAddress, riskTier: req.riskTier, blendedApy }, 'Created yield strategy');
    return strategy;
  }

  public activateYieldStrategy(userAddress: string, strategyId: string, req: ActivateYieldStrategyRequest): YieldFarmingStrategy {
    const strategy = yieldStrategies.get(strategyId);
    if (!strategy) {
      throw new Error('Yield strategy not found');
    }
    if (!userAddress || userAddress !== strategy.userAddress) {
      throw new Error('Unauthorized: strategy does not belong to user');
    }

    const initialTvl = BigInt(req.initialDepositAmount);
    if (initialTvl < MIN_STAKE_AMOUNT) {
      throw new Error(`Initial deposit below minimum: ${MIN_STAKE_AMOUNT.toString()}`);
    }

    const intervalMultiplier = COMPOUND_INTERVAL_MULTIPLIERS[strategy.compoundInterval] ?? 1.0;
    const adjustedApy = Math.round(strategy.blendedApy * intervalMultiplier * 100) / 100;

    const updated: YieldFarmingStrategy = {
      ...strategy,
      status: 'active',
      blendedApy: adjustedApy,
      totalValueLocked: initialTvl.toString(),
      nextCompoundAt: computeNextCompoundAt(strategy.compoundInterval),
      updatedAt: now(),
    };

    yieldStrategies.set(strategyId, updated);
    logger.info({ strategyId, userAddress: strategy.userAddress, initialDeposit: req.initialDepositAmount }, 'Activated yield strategy');
    return updated;
  }

  public compoundStrategy(userAddress: string, strategyId: string): YieldFarmingStrategy {
    const strategy = yieldStrategies.get(strategyId);
    if (!strategy) {
      throw new Error('Yield strategy not found');
    }
    if (!userAddress || userAddress !== strategy.userAddress) {
      throw new Error('Unauthorized: strategy does not belong to user');
    }
    if (strategy.status !== 'active') {
      throw new Error('Strategy is not active');
    }

    const tvl = BigInt(strategy.totalValueLocked);
    const aprDecimal = strategy.blendedApy / 100;
    const dailyRate = aprDecimal / 365;
    const compoundedReward = BigInt(Math.round(Number(tvl) * dailyRate));

    const newTvl = tvl + compoundedReward;
    const updated: YieldFarmingStrategy = {
      ...strategy,
      totalValueLocked: newTvl.toString(),
      nextCompoundAt: computeNextCompoundAt(strategy.compoundInterval),
      updatedAt: now(),
    };

    yieldStrategies.set(strategyId, updated);
    logger.info({ strategyId, compoundedReward: compoundedReward.toString(), newTvl: newTvl.toString() }, 'Compounded yield strategy');
    return updated;
  }

  public getYieldStrategies(userAddress: string): YieldFarmingStrategy[] {
    const results: YieldFarmingStrategy[] = [];
    for (const strategy of yieldStrategies.values()) {
      if (strategy.userAddress === userAddress) {
        results.push(strategy);
      }
    }
    return results;
  }
}

export const stakingService = new StakingService();
