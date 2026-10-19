/**
 * Issue #849 – Reputation Service Unit Tests
 *
 * Comprehensive coverage for the reputation service synchronous helpers
 * (tier definitions, analytics aggregation, leaderboard sorting, cache ops)
 * without requiring live Soroban RPC.
 */

import {
  reputationService,
  TierDefinition,
  ReputationScore,
  DeployerReputationFull,
} from '../services/reputation.service';

const ADDR_A = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
const ADDR_B = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWJF';
const ADDR_C = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAANEW';
const ADDR_DEPLOYER_X = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADPLY';
const ADDR_DEPLOYER_Y = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADPLY2';

function makeUserScore(
  address: string,
  score: number,
  overrides: Partial<ReputationScore> = {},
): ReputationScore {
  const tierIdx = score >= 750 ? 3 : score >= 500 ? 2 : score >= 250 ? 1 : 0;
  const tiers = ['Bronze', 'Silver', 'Gold', 'Platinum'];
  return {
    address,
    total_repayments: 10,
    on_time_repayments: 8,
    defaults: 0,
    total_borrowed: '1000000',
    score,
    tier: tiers[tierIdx],
    last_activity_timestamp: Date.now(),
    fee_discount_bps: [0, 25, 50, 100][tierIdx],
    borrow_limit_multiplier_bps: [10_000, 11_000, 12_500, 15_000][tierIdx],
    participant_type: 'user',
    ...overrides,
  };
}

function makeDeployerFull(
  address: string,
  score: number,
  overrides: Partial<DeployerReputationFull> = {},
): DeployerReputationFull {
  const tierIdx = score >= 750 ? 3 : score >= 500 ? 2 : score >= 250 ? 1 : 0;
  const tiers = ['Bronze', 'Silver', 'Gold', 'Platinum'];
  return {
    address,
    score,
    tier: tiers[tierIdx],
    total_pools_created: 3,
    active_pools: 2,
    total_tvl: '2500000000',
    successful_ops: 10,
    defaults: 0,
    abandoned_pools: 0,
    avg_pool_uptime_bps: 10_000,
    last_activity: Date.now(),
    pools: [
      'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAPOL1',
      'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAPOL2',
    ],
    participant_type: 'deployer',
    ...overrides,
  };
}

beforeEach(() => {
  reputationService.clearCache();
});

// ── Tier definitions ────────────────────────────────────────────────────

describe('Tier definitions', () => {
  let tiers: TierDefinition[];

  beforeAll(() => {
    tiers = reputationService.getTiers();
  });

  it('exposes exactly four tiers in ascending order', () => {
    expect(tiers.map((t) => t.tier)).toEqual([
      'Bronze',
      'Silver',
      'Gold',
      'Platinum',
    ]);
  });

  it('has non-overlapping, ascending score ranges capped at 1000', () => {
    expect(tiers[0].min_score).toBe(0);
    for (let i = 1; i < tiers.length; i++) {
      expect(tiers[i].min_score).toBe(tiers[i - 1].max_score + 1);
    }
    expect(tiers[tiers.length - 1].max_score).toBe(1000);
  });

  it('assigns strictly increasing benefits per tier', () => {
    for (let i = 1; i < tiers.length; i++) {
      const prev = tiers[i - 1].benefits;
      const cur = tiers[i].benefits;
      expect(cur.interest_rate_discount_bps).toBeGreaterThan(
        prev.interest_rate_discount_bps,
      );
      expect(cur.borrowing_limit_multiplier_bps).toBeGreaterThan(
        prev.borrowing_limit_multiplier_bps,
      );
      expect(cur.collateral_reduction_bps).toBeGreaterThan(
        prev.collateral_reduction_bps,
      );
    }
  });

  it('Platinum top-tier has 1% fee discount and 1.5x borrow multiplier', () => {
    const plat = tiers.find((t) => t.tier === 'Platinum')!.benefits;
    expect(plat.interest_rate_discount_bps).toBe(100);
    expect(plat.borrowing_limit_multiplier_bps).toBe(15_000);
  });

  it('getTierBenefits returns the correct slice for a given tier name', () => {
    const gold = reputationService.getTierBenefits('Gold');
    expect(gold).toBeDefined();
    expect(gold!.interest_rate_discount_bps).toBe(50);
  });

  it('getTierBenefits is case-insensitive', () => {
    expect(reputationService.getTierBenefits('silver')).toBeDefined();
    expect(reputationService.getTierBenefits('PLATINUM')).toBeDefined();
  });

  it('getTierBenefits returns undefined for unknown tiers', () => {
    expect(reputationService.getTierBenefits('Diamond')).toBeUndefined();
  });
});

// ── Analytics ───────────────────────────────────────────────────────────

describe('Analytics aggregation', () => {
  it('returns zero-valued analytics on an empty cache', () => {
    const a = reputationService.getAnalytics();
    expect(a.total_tracked).toBe(0);
    expect(a.average_score).toBe(0);
    expect(a.user_count).toBe(0);
    expect(a.deployer_count).toBe(0);
    expect(a.total_pools).toBe(0);
    expect(a.aggregate_tvl).toBe('0');
    expect(Object.keys(a.by_tier)).toEqual(['Bronze', 'Silver', 'Gold', 'Platinum']);
  });

  it('counts users, deployers, pools, and TVL correctly', () => {
    reputationService['clearCache']();
    // Prime caches via the internal maps by accessing the private caches
    // through the public leaderboard and getReputation paths would require
    // the contract to be configured. We therefore push directly via
    // the service's clearCache + manual cache invalidation helpers, and
    // also test directly with the analytics method using pre-seeded data.

    // We simulate entries by leveraging the caches indirectly:
    // Build the expected aggregation manually and verify that the
    // getAnalytics helper works by testing properties without RPC.
    const users = [makeUserScore(ADDR_A, 600), makeUserScore(ADDR_B, 300)];
    const deployers = [
      makeDeployerFull(ADDR_DEPLOYER_X, 800, {
        total_pools_created: 5,
        total_tvl: '10000000000',
      }),
    ];

    // Access the private caches via cast to populate analytics state.
    type WithCaches = {
      leaderboardCache: Map<string, ReputationScore>;
      deployerCache: Map<string, DeployerReputationFull>;
    };
    const svc = reputationService as unknown as WithCaches;
    users.forEach((u) => svc.leaderboardCache.set(u.address, u));
    deployers.forEach((d) => svc.deployerCache.set(d.address, d));

    const a = reputationService.getAnalytics();
    expect(a.total_tracked).toBe(3);
    expect(a.user_count).toBe(2);
    expect(a.deployer_count).toBe(1);
    expect(a.total_pools).toBe(5);
    expect(a.aggregate_tvl).toBe('10000000000');
    expect(a.by_tier['Gold']).toBe(1);
    expect(a.by_tier['Silver']).toBe(1);
    expect(a.by_tier['Platinum']).toBe(1);
    // Average: (600 + 300 + 800) / 3 = 566.666... -> 567 rounded
    expect(a.average_score).toBe(567);
  });

  it('by_tier counts both users and deployers in the same tier', () => {
    type WithCaches = {
      leaderboardCache: Map<string, ReputationScore>;
      deployerCache: Map<string, DeployerReputationFull>;
    };
    const svc = reputationService as unknown as WithCaches;

    svc.leaderboardCache.set(ADDR_A, makeUserScore(ADDR_A, 100)); // Bronze
    svc.leaderboardCache.set(ADDR_B, makeUserScore(ADDR_B, 50)); // Bronze
    svc.deployerCache.set(
      ADDR_DEPLOYER_X,
      makeDeployerFull(ADDR_DEPLOYER_X, 200), // Bronze
    );

    const a = reputationService.getAnalytics();
    expect(a.by_tier['Bronze']).toBe(3);
  });
});

// ── Leaderboard ─────────────────────────────────────────────────────────

describe('Leaderboard sorting and filtering', () => {
  type WithCaches = {
    leaderboardCache: Map<string, ReputationScore>;
    deployerCache: Map<string, DeployerReputationFull>;
  };
  let svc: WithCaches;

  beforeEach(() => {
    svc = reputationService as unknown as WithCaches;
  });

  it('returns empty array for empty caches', () => {
    expect(reputationService.getLeaderboard(10)).toEqual([]);
  });

  it('returns entries sorted by score desc', () => {
    svc.leaderboardCache.set(ADDR_A, makeUserScore(ADDR_A, 100));
    svc.leaderboardCache.set(ADDR_B, makeUserScore(ADDR_B, 500));
    svc.leaderboardCache.set(ADDR_C, makeUserScore(ADDR_C, 300));

    const board = reputationService.getLeaderboard(5);
    expect(board.map((b) => b.address)).toEqual([ADDR_B, ADDR_C, ADDR_A]);
    expect(board.map((b) => b.score)).toEqual([500, 300, 100]);
  });

  it('respects the limit parameter and clamps it', () => {
    for (let i = 0; i < 20; i++) {
      const addr = `ADDR_${i}`;
      svc.leaderboardCache.set(addr, makeUserScore(addr, i * 50));
    }
    expect(reputationService.getLeaderboard(5)).toHaveLength(5);
    expect(reputationService.getLeaderboard(1)).toHaveLength(1);
    expect(reputationService.getLeaderboard(0)).toHaveLength(1); // clamped to 1
    expect(reputationService.getLeaderboard(9999)).toHaveLength(100); // capped at 100
  });

  it('filters by participant type when provided', () => {
    svc.leaderboardCache.set(ADDR_A, makeUserScore(ADDR_A, 400));
    svc.deployerCache.set(
      ADDR_DEPLOYER_X,
      makeDeployerFull(ADDR_DEPLOYER_X, 700),
    );

    const usersOnly = reputationService.getLeaderboard(10, 'user');
    expect(usersOnly.length).toBe(1);
    expect(usersOnly[0].address).toBe(ADDR_A);

    const deployersOnly = reputationService.getLeaderboard(10, 'deployer');
    expect(deployersOnly.length).toBe(1);
    expect(deployersOnly[0].address).toBe(ADDR_DEPLOYER_X);
    expect(deployersOnly[0].participant_type).toBe('deployer');
  });

  it('merges users and deployers when type=all (default)', () => {
    svc.leaderboardCache.set(ADDR_A, makeUserScore(ADDR_A, 400));
    svc.deployerCache.set(
      ADDR_DEPLOYER_X,
      makeDeployerFull(ADDR_DEPLOYER_X, 700),
    );

    const all = reputationService.getLeaderboard(10);
    expect(all.length).toBe(2);
    expect(all[0].score).toBe(700);
    expect(all[1].score).toBe(400);
  });
});

// ── Cache operations ────────────────────────────────────────────────────

describe('Cache management', () => {
  type WithCaches = {
    leaderboardCache: Map<string, ReputationScore>;
    deployerCache: Map<string, DeployerReputationFull>;
    poolCache: Map<string, { pool_address: string }>;
  };
  let svc: WithCaches;

  beforeEach(() => {
    svc = reputationService as unknown as WithCaches;
  });

  it('clearCache empties all three caches', () => {
    svc.leaderboardCache.set(ADDR_A, makeUserScore(ADDR_A, 100));
    svc.deployerCache.set(
      ADDR_DEPLOYER_X,
      makeDeployerFull(ADDR_DEPLOYER_X, 100),
    );
    svc.poolCache.set('POOL1', { pool_address: 'POOL1' });

    reputationService.clearCache();

    expect(svc.leaderboardCache.size).toBe(0);
    expect(svc.deployerCache.size).toBe(0);
    expect(svc.poolCache.size).toBe(0);
  });

  it('invalidateCachesFor removes specific keys', () => {
    svc.leaderboardCache.set(ADDR_A, makeUserScore(ADDR_A, 100));
    svc.deployerCache.set(
      ADDR_DEPLOYER_X,
      makeDeployerFull(ADDR_DEPLOYER_X, 100),
    );
    svc.poolCache.set('POOL1', { pool_address: 'POOL1' });

    reputationService.invalidateCachesFor(ADDR_A, 'POOL1');

    expect(svc.leaderboardCache.has(ADDR_A)).toBe(false);
    expect(svc.deployerCache.has(ADDR_DEPLOYER_X)).toBe(true);
    expect(svc.poolCache.has('POOL1')).toBe(false);
  });

  it('invalidateCachesFor handles undefined args without error', () => {
    expect(() => reputationService.invalidateCachesFor()).not.toThrow();
  });
});

// ── TierBenefits / TierDefinition shape ─────────────────────────────────

describe('Return type contracts (no RPC)', () => {
  it('getDeploymentConfig returns sensible defaults when the contract is unreachable', async () => {
    const cfg = await reputationService.getDeploymentConfig();
    expect(cfg.min_deployer_score).toBeGreaterThanOrEqual(0);
    expect(cfg.min_deployer_score).toBeLessThanOrEqual(1000);
    expect(cfg.max_pools_per_deployer).toBeGreaterThan(0);
    expect(cfg.deploy_cooldown_seconds).toBeGreaterThanOrEqual(0);
    expect(BigInt(cfg.min_initial_deposit)).toBeGreaterThanOrEqual(0n);
  });

  it('getDeployerEligibility returns a shaped object without throwing', async () => {
    const res = await reputationService.checkDeployerEligibility(ADDR_A);
    expect(typeof res.eligible).toBe('boolean');
    if (!res.eligible) {
      expect(typeof res.reason).toBe('string');
    }
  });

  it('getFeeDiscount always returns a number >= 0', async () => {
    const discount = await reputationService.getFeeDiscount(ADDR_A);
    expect(typeof discount).toBe('number');
    expect(discount).toBeGreaterThanOrEqual(0);
  });

  it('getBorrowLimitMultiplier returns 10_000+ basis points', async () => {
    const mult = await reputationService.getBorrowLimitMultiplier(ADDR_A);
    expect(mult).toBeGreaterThanOrEqual(10_000);
  });

  it('getPoolRecord returns a valid empty record for unknown addresses', async () => {
    const rec = await reputationService.getPoolRecord('UNKNOWN_POOL');
    expect(rec.pool_address).toBe('UNKNOWN_POOL');
    expect(rec.is_active).toBe(false);
    expect(rec.performance_score).toBeGreaterThanOrEqual(0);
  });

  it('getReputation returns a complete empty user score for unknown addresses', async () => {
    const s = await reputationService.getReputation(ADDR_A);
    expect(s.address).toBe(ADDR_A);
    expect(s.score).toBe(0);
    expect(s.tier).toBe('Bronze');
    expect(s.participant_type).toBe('user');
    expect(s.fee_discount_bps).toBe(0);
    expect(s.borrow_limit_multiplier_bps).toBe(10_000);
  });

  it('getDeployerReputationFull returns a complete empty deployer record', async () => {
    const d = await reputationService.getDeployerReputationFull(ADDR_DEPLOYER_Y);
    expect(d.address).toBe(ADDR_DEPLOYER_Y);
    expect(d.score).toBe(0);
    expect(d.tier).toBe('Bronze');
    expect(d.participant_type).toBe('deployer');
    expect(Array.isArray(d.pools)).toBe(true);
  });

  it('getDeployerReputation condensed view matches full view', async () => {
    // First populate the deployer cache via getDeployerReputationFull
    await reputationService.getDeployerReputationFull(ADDR_DEPLOYER_Y);
    const condensed = await reputationService.getDeployerReputation(ADDR_DEPLOYER_Y);
    expect(condensed.address).toBe(ADDR_DEPLOYER_Y);
    expect(condensed.participant_type).toBe('deployer');
    expect(condensed.score).toBe(0);
    expect(condensed.tier).toBe('Bronze');
  });
});
