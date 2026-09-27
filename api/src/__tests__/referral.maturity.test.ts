import { referralService, ReferrerStats } from '../services/referral.service';

const DAY = 24 * 60 * 60 * 1000;
const T0 = Date.UTC(2026, 0, 1);

describe('referral reward maturity window (#998)', () => {
  let now = T0;
  let nowSpy: jest.SpyInstance<number, []>;

  beforeEach(() => {
    now = T0;
    nowSpy = jest.spyOn(Date, 'now').mockImplementation(() => now);
  });

  afterEach(() => {
    nowSpy.mockRestore();
  });

  const advanceDays = (days: number): void => {
    now += days * DAY;
  };

  const statsOf = (address: string): ReferrerStats => {
    const s = referralService.getStats(address);
    if (!s) throw new Error(`no stats for ${address}`);
    return s;
  };

  const setupAccrual = (referrer: string, referee: string, fee = 1000): void => {
    const code = referralService.generateCode(referrer);
    referralService.register(referee, code);
    referralService.accrueFee(referee, fee);
  };

  it('rejects the first claim before rewards are 30 days old', () => {
    setupAccrual('R_FIRST', 'A_FIRST');

    expect(() => referralService.claim('R_FIRST')).toThrow('30-day maturity period not reached');

    const s = statsOf('R_FIRST');
    expect(s.claimable).toBe(100);
    expect(s.totalClaimed).toBe(0);
    expect(s.pendingTranches).toHaveLength(1);
  });

  it('batch distribution pays nothing before maturity', () => {
    setupAccrual('R_BATCH_IMMATURE', 'A_BATCH_IMMATURE');

    const d = referralService.distributeRewards(['R_BATCH_IMMATURE']);
    expect(d.distributedCount).toBe(0);
    expect(d.totalDistributedAmount).toBe(0);

    const s = statsOf('R_BATCH_IMMATURE');
    expect(s.claimable).toBe(100);
    expect(s.totalClaimed).toBe(0);
    expect(s.pendingTranches).toHaveLength(1);
  });

  it('pays only the matured tranche when old and new accruals are mixed', () => {
    setupAccrual('R_MIXED', 'A_MIXED'); // t0, 100
    advanceDays(31);
    referralService.accrueFee('A_MIXED', 1000); // t0+31d, 100
    advanceDays(1); // t0+32d: first tranche is 32d old, second is 1d old

    const res = referralService.claim('R_MIXED');
    expect(res.amount).toBe(100);

    const s = statsOf('R_MIXED');
    expect(s.claimable).toBe(100);
    expect(s.totalClaimed).toBe(100);
    expect(s.pendingTranches).toHaveLength(1);
  });

  it('succeeds at the exact maturity boundary', () => {
    setupAccrual('R_BOUNDARY', 'A_BOUNDARY');
    advanceDays(30);

    const res = referralService.claim('R_BOUNDARY');
    expect(res.amount).toBe(100);

    const s = statsOf('R_BOUNDARY');
    expect(s.claimable).toBe(0);
    expect(s.totalClaimed).toBe(100);
    expect(s.pendingTranches).toHaveLength(0);
  });

  it('keeps fresh accruals locked even after the previous claim cooldown elapsed', () => {
    setupAccrual('R_COOLDOWN', 'A_COOLDOWN'); // t0
    advanceDays(30);
    expect(referralService.claim('R_COOLDOWN').amount).toBe(100); // lastClaimAt = t0+30d

    advanceDays(15); // t0+45d
    referralService.accrueFee('A_COOLDOWN', 1000); // new 100 accrued
    advanceDays(16); // t0+61d: claim cooldown (31d) has passed

    expect(() => referralService.claim('R_COOLDOWN')).toThrow('30-day maturity period not reached');
    expect(statsOf('R_COOLDOWN').claimable).toBe(100);

    advanceDays(14); // t0+75d: the t0+45d tranche is now exactly 30d old
    expect(referralService.claim('R_COOLDOWN').amount).toBe(100);
  });

  it('batch distribution pays matured users and skips immature ones', () => {
    setupAccrual('R_BATCH_OLD', 'A_BATCH_OLD'); // t0
    advanceDays(20);
    setupAccrual('R_BATCH_NEW', 'A_BATCH_NEW'); // t0+20d
    advanceDays(15); // t0+35d: old is 35d mature, new is 15d immature

    const d = referralService.distributeRewards(['R_BATCH_OLD', 'R_BATCH_NEW']);
    expect(d.distributedCount).toBe(1);
    expect(d.totalDistributedAmount).toBe(100);

    expect(statsOf('R_BATCH_OLD').claimable).toBe(0);
    expect(statsOf('R_BATCH_OLD').totalClaimed).toBe(100);
    expect(statsOf('R_BATCH_NEW').claimable).toBe(100);
    expect(statsOf('R_BATCH_NEW').totalClaimed).toBe(0);
  });

  it('matures L2 commissions on the same schedule as L1 shares', () => {
    const topCode = referralService.generateCode('R_L2_TOP');
    referralService.register('R_L2_MID', topCode);
    const midCode = referralService.generateCode('R_L2_MID');
    referralService.register('A_L2', midCode);
    referralService.accrueFee('A_L2', 1000); // mid gets 100 (L1), top gets 30 (L2)

    expect(statsOf('R_L2_TOP').claimable).toBe(30);

    advanceDays(30);
    expect(referralService.claim('R_L2_TOP').amount).toBe(30);
    expect(referralService.claim('R_L2_MID').amount).toBe(100);
  });
});
