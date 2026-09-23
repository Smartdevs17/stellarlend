import {
  Contract,
  Address,
  TransactionBuilder,
  scValToNative,
  xdr,
} from '@stellar/stellar-sdk';
import { Server as SorobanServer } from '@stellar/stellar-sdk/rpc';
import { config } from '../config';
import logger from '../utils/logger';

const REPUTATION_CONTRACT_ID = process.env.REPUTATION_CONTRACT_ID ?? '';
const TIER_NAMES = ['Bronze', 'Silver', 'Gold', 'Platinum'];
const PARTICIPANT_TYPES = ['user', 'deployer'] as const;
type ParticipantType = typeof PARTICIPANT_TYPES[number];

export interface ReputationScore {
  address: string;
  total_repayments: number;
  on_time_repayments: number;
  defaults: number;
  total_borrowed: string;
  score: number;
  tier: string;
  last_activity_timestamp: number;
  fee_discount_bps: number;
  borrow_limit_multiplier_bps: number;
  participant_type?: ParticipantType;
}

export interface DeployerReputationFull {
  address: string;
  score: number;
  tier: string;
  total_pools_created: number;
  active_pools: number;
  total_tvl: string;
  successful_ops: number;
  defaults: number;
  abandoned_pools: number;
  avg_pool_uptime_bps: number;
  last_activity: number;
  pools: string[];
  participant_type: 'deployer';
}

export interface DeployerPoolRecord {
  pool_address: string;
  created_at: number;
  tvl: string;
  active_borrowers: number;
  liquidation_events: number;
  performance_score: number;
  is_active: boolean;
}

export interface PoolDeploymentConfig {
  min_deployer_score: number;
  max_pools_per_deployer: number;
  deploy_cooldown_seconds: number;
  min_initial_deposit: string;
}

export interface TierBenefits {
  interest_rate_discount_bps: number;
  borrowing_limit_multiplier_bps: number;
  collateral_reduction_bps: number;
}

export interface TierDefinition {
  tier: string;
  min_score: number;
  max_score: number;
  benefits: TierBenefits;
}

export interface ReputationAnalytics {
  total_tracked: number;
  average_score: number;
  by_tier: Record<string, number>;
  deployer_count: number;
  user_count: number;
  total_pools: number;
  aggregate_tvl: string;
}

const leaderboardCache = new Map<string, ReputationScore>();
const deployerCache = new Map<string, DeployerReputationFull>();
const poolCache = new Map<string, DeployerPoolRecord>();

const TIER_DEFINITIONS: TierDefinition[] = [
  {
    tier: 'Bronze',
    min_score: 0,
    max_score: 249,
    benefits: {
      interest_rate_discount_bps: 0,
      borrowing_limit_multiplier_bps: 10_000,
      collateral_reduction_bps: 0,
    },
  },
  {
    tier: 'Silver',
    min_score: 250,
    max_score: 499,
    benefits: {
      interest_rate_discount_bps: 25,
      borrowing_limit_multiplier_bps: 11_000,
      collateral_reduction_bps: 100,
    },
  },
  {
    tier: 'Gold',
    min_score: 500,
    max_score: 749,
    benefits: {
      interest_rate_discount_bps: 50,
      borrowing_limit_multiplier_bps: 12_500,
      collateral_reduction_bps: 200,
    },
  },
  {
    tier: 'Platinum',
    min_score: 750,
    max_score: 1000,
    benefits: {
      interest_rate_discount_bps: 100,
      borrowing_limit_multiplier_bps: 15_000,
      collateral_reduction_bps: 300,
    },
  },
];

class ReputationService {
  private server: SorobanServer;

  constructor() {
    this.server = new SorobanServer(config.stellar.sorobanRpcUrl);
  }

  private async simulateCall(
    method: string,
    args: xdr.ScVal[],
  ): Promise<unknown | null> {
    if (!REPUTATION_CONTRACT_ID) {
      return null;
    }
    try {
      const contract = new Contract(REPUTATION_CONTRACT_ID);
      const account = await this.server.getAccount(config.stellar.readOnlySimulationAccount);
      const tx = new TransactionBuilder(account, {
        fee: '100',
        networkPassphrase: config.stellar.networkPassphrase,
      })
        .addOperation(contract.call(method, ...args))
        .setTimeout(30)
        .build();

      const sim = await this.server.simulateTransaction(tx);
      if (!('result' in sim) || !sim.result) {
        return null;
      }
      return scValToNative(sim.result.retval);
    } catch (err) {
      logger.warn(`Reputation contract simulation failed for ${method}`, {
        method,
        err: String(err),
      });
      return null;
    }
  }

  async getReputation(address: string): Promise<ReputationScore> {
    const result = await this.simulateCall('get_user_reputation', [
      new Address(address).toScVal(),
    ]);

    if (!result || typeof result !== 'object') {
      return emptyUserScore(address);
    }

    const raw = result as Record<string, unknown>;
    const score: ReputationScore = {
      address,
      total_repayments: Number(raw['total_repayments'] ?? 0),
      on_time_repayments: Number(raw['on_time_repayments'] ?? 0),
      defaults: Number(raw['defaults'] ?? 0),
      total_borrowed: String(raw['total_borrowed'] ?? '0'),
      score: Number(raw['score'] ?? 0),
      tier: TIER_NAMES[Number(raw['tier'] ?? 0)] ?? 'Bronze',
      last_activity_timestamp: Number(raw['last_activity'] ?? raw['last_activity_timestamp'] ?? 0),
      fee_discount_bps: Number(raw['fee_discount_bps'] ?? 0),
      borrow_limit_multiplier_bps:
        Number(raw['borrow_limit_multiplier_bps'] ?? 10_000),
      participant_type: 'user',
    };

    leaderboardCache.set(address, score);
    return score;
  }

  async getDeployerReputation(address: string): Promise<ReputationScore> {
    const full = await this.getDeployerReputationFull(address);
    return {
      address,
      total_repayments: full.successful_ops,
      on_time_repayments: full.successful_ops,
      defaults: full.defaults,
      total_borrowed: '0',
      score: full.score,
      tier: full.tier,
      last_activity_timestamp: full.last_activity,
      fee_discount_bps: 0,
      borrow_limit_multiplier_bps: 10_000,
      participant_type: 'deployer',
    };
  }

  async getDeployerReputationFull(address: string): Promise<DeployerReputationFull> {
    const cached = deployerCache.get(address);
    if (cached) {
      return cached;
    }

    const result = await this.simulateCall('get_deployer_reputation_full', [
      new Address(address).toScVal(),
    ]);

    if (!result || typeof result !== 'object') {
      return emptyDeployerFull(address);
    }

    const raw = result as Record<string, unknown>;
    const poolsList = Array.isArray(raw['pools'])
      ? (raw['pools'] as unknown[]).map((p) => String(p))
      : [];

    const full: DeployerReputationFull = {
      address,
      score: Number(raw['score'] ?? 0),
      tier: TIER_NAMES[Number(raw['tier'] ?? 0)] ?? 'Bronze',
      total_pools_created: Number(raw['total_pools_created'] ?? 0),
      active_pools: Number(raw['active_pools'] ?? 0),
      total_tvl: String(raw['total_tvl'] ?? '0'),
      successful_ops: Number(raw['successful_ops'] ?? 0),
      defaults: Number(raw['defaults'] ?? 0),
      abandoned_pools: Number(raw['abandoned_pools'] ?? 0),
      avg_pool_uptime_bps: Number(raw['avg_pool_uptime_bps'] ?? 10_000),
      last_activity: Number(raw['last_activity'] ?? 0),
      pools: poolsList,
      participant_type: 'deployer',
    };

    deployerCache.set(address, full);
    return full;
  }

  async getPoolRecord(poolAddress: string): Promise<DeployerPoolRecord> {
    const cached = poolCache.get(poolAddress);
    if (cached) {
      return cached;
    }

    const result = await this.simulateCall('get_pool_record', [
      new Address(poolAddress).toScVal(),
    ]);

    if (!result || typeof result !== 'object') {
      return emptyPoolRecord(poolAddress);
    }

    const raw = result as Record<string, unknown>;
    const record: DeployerPoolRecord = {
      pool_address: poolAddress,
      created_at: Number(raw['created_at'] ?? 0),
      tvl: String(raw['tvl'] ?? '0'),
      active_borrowers: Number(raw['active_borrowers'] ?? 0),
      liquidation_events: Number(raw['liquidation_events'] ?? 0),
      performance_score: Number(raw['performance_score'] ?? 0),
      is_active: Boolean(raw['is_active'] ?? false),
    };

    poolCache.set(poolAddress, record);
    return record;
  }

  async getDeploymentConfig(): Promise<PoolDeploymentConfig> {
    const result = await this.simulateCall('reputation_get_deployment_config', []);

    if (!result || typeof result !== 'object') {
      return {
        min_deployer_score: 100,
        max_pools_per_deployer: 10,
        deploy_cooldown_seconds: 3600,
        min_initial_deposit: '1000000',
      };
    }

    const raw = result as Record<string, unknown>;
    return {
      min_deployer_score: Number(raw['min_deployer_score'] ?? 100),
      max_pools_per_deployer: Number(raw['max_pools_per_deployer'] ?? 10),
      deploy_cooldown_seconds: Number(raw['deploy_cooldown_seconds'] ?? 3600),
      min_initial_deposit: String(raw['min_initial_deposit'] ?? '1000000'),
    };
  }

  async checkDeployerEligibility(address: string): Promise<{ eligible: boolean; reason?: string }> {
    const result = await this.simulateCall('check_deployer_eligibility', [
      new Address(address).toScVal(),
    ]);

    if (result === null) {
      const config = await this.getDeploymentConfig();
      const rep = await this.getDeployerReputationFull(address);
      if (rep.total_pools_created === 0) {
        return { eligible: true };
      }
      if (rep.score < config.min_deployer_score) {
        return {
          eligible: false,
          reason: `Insufficient reputation. Required: ${config.min_deployer_score}, Current: ${rep.score}`,
        };
      }
      if (rep.total_pools_created >= config.max_pools_per_deployer) {
        return {
          eligible: false,
          reason: `Maximum pools (${config.max_pools_per_deployer}) reached`,
        };
      }
      return { eligible: true };
    }

    return { eligible: Boolean(result) };
  }

  async getFeeDiscount(address: string): Promise<number> {
    const result = await this.simulateCall('get_reputation_fee_discount', [
      new Address(address).toScVal(),
    ]);
    return result !== null ? Number(result) : 0;
  }

  async getBorrowLimitMultiplier(address: string): Promise<number> {
    const result = await this.simulateCall('get_reputation_borrow_limit_multiplier', [
      new Address(address).toScVal(),
    ]);
    return result !== null ? Number(result) : 10_000;
  }

  getTiers(): TierDefinition[] {
    return TIER_DEFINITIONS;
  }

  getTierBenefits(tierName: string): TierBenefits | undefined {
    const tier = TIER_DEFINITIONS.find(
      (t) => t.tier.toLowerCase() === tierName.toLowerCase(),
    );
    return tier?.benefits;
  }

  getAnalytics(): ReputationAnalytics {
    const userEntries = Array.from(leaderboardCache.values()).filter(
      (e) => e.participant_type !== 'deployer',
    );
    const deployerEntries = Array.from(deployerCache.values());

    const byTier = TIER_NAMES.reduce<Record<string, number>>((acc, tier) => {
      acc[tier] =
        userEntries.filter((e) => e.tier === tier).length +
        deployerEntries.filter((e) => e.tier === tier).length;
      return acc;
    }, {});

    const totalScores = [
      ...userEntries.map((e) => e.score),
      ...deployerEntries.map((e) => e.score),
    ];
    const average_score = totalScores.length
      ? Math.round(totalScores.reduce((s, a) => s + a, 0) / totalScores.length)
      : 0;

    const aggregateTvl = deployerEntries.reduce((sum, d) => {
      const tvl = BigInt(d.total_tvl || '0');
      return sum + tvl;
    }, BigInt(0));

    const totalPools = deployerEntries.reduce(
      (sum, d) => sum + d.total_pools_created,
      0,
    );

    return {
      total_tracked: userEntries.length + deployerEntries.length,
      average_score,
      by_tier: byTier,
      deployer_count: deployerEntries.length,
      user_count: userEntries.length,
      total_pools: totalPools,
      aggregate_tvl: aggregateTvl.toString(),
    };
  }

  getLeaderboard(limit: number, type?: ParticipantType): ReputationScore[] {
    let entries: ReputationScore[] = [];

    if (!type || type === 'user') {
      entries = entries.concat(Array.from(leaderboardCache.values()));
    }
    if (!type || type === 'deployer') {
      const deployerScores: ReputationScore[] = Array.from(
        deployerCache.values(),
      ).map((d) => ({
        address: d.address,
        total_repayments: d.successful_ops,
        on_time_repayments: d.successful_ops,
        defaults: d.defaults,
        total_borrowed: '0',
        score: d.score,
        tier: d.tier,
        last_activity_timestamp: d.last_activity,
        fee_discount_bps: 0,
        borrow_limit_multiplier_bps: 10_000,
        participant_type: 'deployer',
      }));
      entries = entries.concat(deployerScores);
    }

    return entries.sort((a, b) => b.score - a.score).slice(0, limit);
  }

  async applyDecay(address: string, isDeployer: boolean): Promise<void> {
    await this.simulateCall('reputation_apply_decay', [
      new Address(address).toScVal(),
      xdr.ScVal.fromScvBool(isDeployer),
    ]);
  }

  clearCache(): void {
    leaderboardCache.clear();
    deployerCache.clear();
    poolCache.clear();
  }

  invalidateCachesFor(address?: string, poolAddress?: string): void {
    if (address) {
      leaderboardCache.delete(address);
      deployerCache.delete(address);
    }
    if (poolAddress) {
      poolCache.delete(poolAddress);
    }
  }
}

function emptyUserScore(address: string): ReputationScore {
  return {
    address,
    total_repayments: 0,
    on_time_repayments: 0,
    defaults: 0,
    total_borrowed: '0',
    score: 0,
    tier: 'Bronze',
    last_activity_timestamp: 0,
    fee_discount_bps: 0,
    borrow_limit_multiplier_bps: 10_000,
    participant_type: 'user',
  };
}

function emptyDeployerFull(address: string): DeployerReputationFull {
  return {
    address,
    score: 0,
    tier: 'Bronze',
    total_pools_created: 0,
    active_pools: 0,
    total_tvl: '0',
    successful_ops: 0,
    defaults: 0,
    abandoned_pools: 0,
    avg_pool_uptime_bps: 10_000,
    last_activity: 0,
    pools: [],
    participant_type: 'deployer',
  };
}

function emptyPoolRecord(poolAddress: string): DeployerPoolRecord {
  return {
    pool_address: poolAddress,
    created_at: 0,
    tvl: '0',
    active_borrowers: 0,
    liquidation_events: 0,
    performance_score: 0,
    is_active: false,
  };
}

export const reputationService = new ReputationService();
