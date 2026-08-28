import crypto from 'crypto';
import logger from '../utils/logger';

export interface ReferralRecord {
  referrerAddress: string;
  refereeAddress: string;
  code: string;
  registeredAt: number;
  totalFeesGenerated: number;
  referrerEarned: number;
}

export interface ReferrerStats {
  code: string;
  totalReferrals: number;
  l2Referrals: number;
  totalEarned: number;
  totalClaimed: number;
  claimable: number;
  lastClaimAt: number;
  referees: string[];
  tier: number;
  totalDeposit: number;
}

export interface ReferralProgramConfig {
  l1FeeSharePct: number;
  l2FeeSharePct: number;
  maturityDays: number;
  tier1Threshold: number;
  tier1BonusBps: number;
  tier2Threshold: number;
  tier2BonusBps: number;
  tier3Threshold: number;
  tier3BonusBps: number;
  minDepositQualify: number;
  autoDistributeEnabled: boolean;
}

export interface RewardDistributionRecord {
  id: string;
  userAddress: string;
  amount: number;
  asset: string;
  distributedAt: number;
  txHash: string;
  batchId?: string;
  status: 'completed' | 'pending';
}

export interface LeaderboardEntry {
  rank: number;
  userAddress: string;
  maskedAddress: string;
  code: string;
  totalReferrals: number;
  l2Referrals: number;
  totalEarned: number;
  claimable: number;
  tier: number;
  tierLabel: string;
}

export interface GlobalReferralAnalytics {
  totalAffiliates: number;
  totalReferees: number;
  totalFeesGenerated: number;
  totalRewardsDistributed: number;
  claimableBalanceProtocol: number;
  conversionRate: string;
  tierDistribution: Record<string, number>;
  averageEarnedPerAffiliate: number;
}

let PROGRAM_CONFIG: ReferralProgramConfig = {
  l1FeeSharePct: 10,
  l2FeeSharePct: 3,
  maturityDays: 30,
  tier1Threshold: 5,
  tier1BonusBps: 100, // +1%
  tier2Threshold: 20,
  tier2BonusBps: 300, // +3%
  tier3Threshold: 50,
  tier3BonusBps: 500, // +5%
  minDepositQualify: 100 * 10 ** 7, // 100 tokens
  autoDistributeEnabled: true,
};

const codes = new Map<string, string>(); // userAddress -> code
const codeToAddress = new Map<string, string>(); // code -> userAddress
const referrals = new Map<string, ReferralRecord>(); // refereeAddress -> record
const stats = new Map<string, ReferrerStats>();
const distributions: RewardDistributionRecord[] = [];

function generateUniqueCode(address: string): string {
  const hash = crypto.createHash('sha256').update(address + Date.now()).digest('hex');
  return hash.slice(0, 8).toUpperCase();
}

function maskAddress(addr: string): string {
  if (addr.length <= 8) return addr;
  return `${addr.substring(0, 4)}...${addr.substring(addr.length - 4)}`;
}

export const referralService = {
  generateCode(userAddress: string): string {
    const existing = codes.get(userAddress);
    if (existing) return existing;

    const code = generateUniqueCode(userAddress);
    codes.set(userAddress, code);
    codeToAddress.set(code, userAddress);

    if (!stats.has(userAddress)) {
      stats.set(userAddress, {
        code,
        totalReferrals: 0,
        l2Referrals: 0,
        totalEarned: 0,
        totalClaimed: 0,
        claimable: 0,
        lastClaimAt: 0,
        referees: [],
        tier: 0,
        totalDeposit: 0,
      });
    }

    logger.info(`Referral code generated: ${code} for ${userAddress}`);
    return code;
  },

  register(refereeAddress: string, referralCode: string): { referrer: string } {
    const referrerAddress = codeToAddress.get(referralCode);
    if (!referrerAddress) throw new Error('Invalid referral code');
    if (referrerAddress === refereeAddress) throw new Error('Self-referral not allowed');
    if (referrals.has(refereeAddress)) throw new Error('Already registered with a referral');

    const record: ReferralRecord = {
      referrerAddress,
      refereeAddress,
      code: referralCode,
      registeredAt: Date.now(),
      totalFeesGenerated: 0,
      referrerEarned: 0,
    };
    referrals.set(refereeAddress, record);

    const referrerStats = stats.get(referrerAddress)!;
    referrerStats.totalReferrals++;
    referrerStats.referees.push(refereeAddress);
    referrerStats.tier = this.calculateTier(referrerStats.totalReferrals);

    // L2: check if referrer was also referred
    const referrerRecord = referrals.get(referrerAddress);
    if (referrerRecord) {
      const l1Stats = stats.get(referrerRecord.referrerAddress);
      if (l1Stats) l1Stats.l2Referrals++;
    }

    logger.info(`Referral registered: ${refereeAddress} -> ${referrerAddress}`);
    return { referrer: referrerAddress };
  },

  accrueFee(refereeAddress: string, feeAmount: number): void {
    const record = referrals.get(refereeAddress);
    if (!record) return;

    const l1Share = (feeAmount * PROGRAM_CONFIG.l1FeeSharePct) / 100;
    record.totalFeesGenerated += feeAmount;
    record.referrerEarned += l1Share;

    const referrerStats = stats.get(record.referrerAddress);
    if (referrerStats) {
      referrerStats.totalEarned += l1Share;
      referrerStats.claimable += l1Share;
    }

    // L2 commission
    const l1Record = referrals.get(record.referrerAddress);
    if (l1Record) {
      const l2Share = (feeAmount * PROGRAM_CONFIG.l2FeeSharePct) / 100;
      const l1Stats = stats.get(l1Record.referrerAddress);
      if (l1Stats) {
        l1Stats.totalEarned += l2Share;
        l1Stats.claimable += l2Share;
      }
    }
  },

  getStats(userAddress: string): ReferrerStats | null {
    return stats.get(userAddress) ?? null;
  },

  claim(userAddress: string): { amount: number; txHash: string } {
    const s = stats.get(userAddress);
    if (!s || s.claimable <= 0) throw new Error('Nothing to claim');

    const maturityMs = PROGRAM_CONFIG.maturityDays * 24 * 60 * 60 * 1000;
    const now = Date.now();
    if (s.lastClaimAt > 0 && now - s.lastClaimAt < maturityMs) {
      throw new Error(`${PROGRAM_CONFIG.maturityDays}-day maturity period not reached`);
    }

    const amount = s.claimable;
    s.totalClaimed += amount;
    s.claimable = 0;
    s.lastClaimAt = now;

    const txHash = `dist_tx_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`;
    distributions.push({
      id: `dist_${Date.now()}`,
      userAddress,
      amount,
      asset: 'USDC',
      distributedAt: now,
      txHash,
      status: 'completed',
    });

    logger.info(`Referral claim: ${userAddress} claimed ${amount}`);
    return { amount, txHash };
  },

  /**
   * Distribute rewards to affiliates (batch or single)
   */
  distributeRewards(userAddresses?: string[]): {
    distributedCount: number;
    totalDistributedAmount: number;
    distributions: RewardDistributionRecord[];
  } {
    const targets = userAddresses && userAddresses.length > 0
      ? userAddresses
      : Array.from(stats.keys());

    const batchId = `batch_${Date.now()}`;
    const batchDistributions: RewardDistributionRecord[] = [];
    let totalDistributedAmount = 0;

    for (const address of targets) {
      const userStat = stats.get(address);
      if (!userStat || userStat.claimable <= 0) continue;

      const amount = userStat.claimable;
      userStat.totalClaimed += amount;
      userStat.claimable = 0;
      userStat.lastClaimAt = Date.now();

      const record: RewardDistributionRecord = {
        id: `dist_${Date.now()}_${Math.random().toString(36).substring(2, 6)}`,
        userAddress: address,
        amount,
        asset: 'USDC',
        distributedAt: Date.now(),
        txHash: `dist_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`,
        batchId,
        status: 'completed',
      };

      distributions.push(record);
      batchDistributions.push(record);
      totalDistributedAmount += amount;
    }

    logger.info(`Affiliate rewards distributed: ${batchDistributions.length} payouts totaling ${totalDistributedAmount}`);
    return {
      distributedCount: batchDistributions.length,
      totalDistributedAmount,
      distributions: batchDistributions,
    };
  },

  /**
   * Get distribution history
   */
  getDistributionHistory(userAddress?: string): RewardDistributionRecord[] {
    if (userAddress) {
      return distributions.filter((d) => d.userAddress === userAddress);
    }
    return [...distributions].reverse();
  },

  /**
   * Leaderboard: rank top referrers by earnings or total referrals
   */
  getLeaderboard(limit = 10, sortBy: 'totalEarned' | 'totalReferrals' | 'claimable' = 'totalEarned'): LeaderboardEntry[] {
    const list: { address: string; stat: ReferrerStats }[] = [];
    for (const [address, stat] of stats.entries()) {
      list.push({ address, stat });
    }

    list.sort((a, b) => b.stat[sortBy] - a.stat[sortBy]);

    return list.slice(0, limit).map((item, idx) => {
      const tier = this.calculateTier(item.stat.totalReferrals);
      let tierLabel = 'Bronze';
      if (tier === 3) tierLabel = 'Diamond';
      else if (tier === 2) tierLabel = 'Gold';
      else if (tier === 1) tierLabel = 'Silver';

      return {
        rank: idx + 1,
        userAddress: item.address,
        maskedAddress: maskAddress(item.address),
        code: item.stat.code,
        totalReferrals: item.stat.totalReferrals,
        l2Referrals: item.stat.l2Referrals,
        totalEarned: item.stat.totalEarned,
        claimable: item.stat.claimable,
        tier,
        tierLabel,
      };
    });
  },

  /**
   * Global referral program analytics
   */
  getGlobalAnalytics(): GlobalReferralAnalytics {
    let totalFeesGenerated = 0;
    let totalRewardsDistributed = 0;
    let claimableBalanceProtocol = 0;
    let totalRefereesCount = referrals.size;

    const tierDistribution = { Bronze: 0, Silver: 0, Gold: 0, Diamond: 0 };

    for (const stat of stats.values()) {
      totalFeesGenerated += stat.totalEarned * (100 / PROGRAM_CONFIG.l1FeeSharePct);
      totalRewardsDistributed += stat.totalClaimed;
      claimableBalanceProtocol += stat.claimable;

      const tier = this.calculateTier(stat.totalReferrals);
      if (tier === 3) tierDistribution.Diamond++;
      else if (tier === 2) tierDistribution.Gold++;
      else if (tier === 1) tierDistribution.Silver++;
      else tierDistribution.Bronze++;
    }

    const totalAffiliates = stats.size;
    const conversionRate = totalAffiliates > 0
      ? ((totalRefereesCount / Math.max(1, totalAffiliates * 3)) * 100).toFixed(1)
      : '0.0';

    const averageEarnedPerAffiliate = totalAffiliates > 0
      ? Math.round((totalRewardsDistributed / totalAffiliates) * 100) / 100
      : 0;

    return {
      totalAffiliates,
      totalReferees: totalRefereesCount,
      totalFeesGenerated: Math.round(totalFeesGenerated * 100) / 100,
      totalRewardsDistributed: Math.round(totalRewardsDistributed * 100) / 100,
      claimableBalanceProtocol: Math.round(claimableBalanceProtocol * 100) / 100,
      conversionRate: `${conversionRate}%`,
      tierDistribution,
      averageEarnedPerAffiliate,
    };
  },

  /**
   * Get and update program configuration
   */
  getConfig(): ReferralProgramConfig {
    return { ...PROGRAM_CONFIG };
  },

  updateConfig(newConfig: Partial<ReferralProgramConfig>): ReferralProgramConfig {
    PROGRAM_CONFIG = {
      ...PROGRAM_CONFIG,
      ...newConfig,
    };
    logger.info('Referral program configuration updated', PROGRAM_CONFIG);
    return { ...PROGRAM_CONFIG };
  },

  getReferralLink(userAddress: string): string {
    const code = codes.get(userAddress);
    if (!code) throw new Error('No referral code found. Generate one first.');
    return `https://stellarlend.com?ref=${code}`;
  },

  calculateTier(totalReferrals: number): number {
    if (totalReferrals >= PROGRAM_CONFIG.tier3Threshold) return 3;
    if (totalReferrals >= PROGRAM_CONFIG.tier2Threshold) return 2;
    if (totalReferrals >= PROGRAM_CONFIG.tier1Threshold) return 1;
    return 0;
  },

  getTierBonus(tier: number): number {
    if (tier === 3) return PROGRAM_CONFIG.tier3BonusBps;
    if (tier === 2) return PROGRAM_CONFIG.tier2BonusBps;
    if (tier === 1) return PROGRAM_CONFIG.tier1BonusBps;
    return 0;
  },

  validateAntiSybil(userAddress: string, totalDeposit: number): boolean {
    return totalDeposit >= PROGRAM_CONFIG.minDepositQualify;
  },

  getConversionFunnel(userAddress: string) {
    const s = stats.get(userAddress);
    if (!s) return null;

    return {
      referralCode: s.code,
      referralsGenerated: s.referees.length,
      referralsConverted: s.totalReferrals,
      conversionRate: s.referees.length > 0 ? (s.totalReferrals / s.referees.length * 100).toFixed(2) : '0',
      l2Referrals: s.l2Referrals,
    };
  },

  getAntiSybilStatus(userAddress: string, totalDeposit: number) {
    const isEligible = this.validateAntiSybil(userAddress, totalDeposit);
    const tier = this.calculateTier(stats.get(userAddress)?.totalReferrals ?? 0);

    return {
      isEligible,
      totalDeposit,
      minRequired: PROGRAM_CONFIG.minDepositQualify,
      currentTier: tier,
      tierBonus: this.getTierBonus(tier),
    };
  },
};
