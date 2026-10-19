import { Keypair } from '@stellar/stellar-sdk';
import request from 'supertest';
import app from '../app';
import { StellarService } from '../services/stellar.service';
import { redisCacheService } from '../services/redisCache.service';
import { readCacheService } from '../services/readCache.service';

const address = Keypair.fromRawEd25519Seed(Buffer.alloc(32, 1)).publicKey();
const route = `/api/v1/lending/positions/${address}/health`;
const summary = {
  total_collateral_value: 1250000n,
  weighted_collateral_value: 1000000n,
  total_debt_value: 1000000n,
  weighted_debt_value: 1000000n,
  health_factor: 10000n,
  is_liquidatable: false,
  borrow_capacity: 0n,
};

describe('GET position health (#1413)', () => {
  let simulate: jest.SpyInstance;

  beforeEach(() => {
    redisCacheService.clearAllForTests();
    readCacheService.resetForTests();
    simulate = jest.spyOn(StellarService.prototype as any, 'simulateContractCall')
      .mockResolvedValue(summary);
  });

  afterEach(() => {
    simulate.mockRestore();
  });

  it('returns oracle-weighted on-chain health at the liquidation boundary', async () => {
    const response = await request(app).get(route);
    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      success: true,
      data: {
        userAddress: address,
        totalCollateralValue: '1250000',
        weightedCollateralValue: '1000000',
        totalDebtValue: '1000000',
        weightedDebtValue: '1000000',
        healthFactor: '10000',
        isLiquidatable: false,
        borrowCapacity: '0',
      },
    });
    expect(simulate).toHaveBeenCalledTimes(1);
  });

  it('returns liquidatable status verbatim from the contract', async () => {
    simulate.mockResolvedValueOnce({
      ...summary, health_factor: 9999n, is_liquidatable: true,
    });
    const response = await request(app).get(route);
    expect(response.status).toBe(200);
    expect(response.body.data.healthFactor).toBe('9999');
    expect(response.body.data.isLiquidatable).toBe(true);
  });

  it('serves repeated queries from the position cache', async () => {
    expect((await request(app).get(route)).status).toBe(200);
    expect((await request(app).get(route)).status).toBe(200);
    expect(simulate).toHaveBeenCalledTimes(1);
  });

  it('refreshes after position cache invalidation', async () => {
    await request(app).get(route);
    await readCacheService.invalidate('position');
    await request(app).get(route);
    expect(simulate).toHaveBeenCalledTimes(2);
  });

  it('rejects invalid addresses before attempting the contract read', async () => {
    const response = await request(app).get('/api/v1/lending/positions/invalid/health');
    expect(response.status).toBe(400);
    expect(simulate).not.toHaveBeenCalled();
  });

  it('does not cache incomplete summaries', async () => {
    simulate.mockResolvedValueOnce({ health_factor: 10000n });
    expect((await request(app).get(route)).status).toBe(500);
    expect((await request(app).get(route)).status).toBe(200);
    expect(simulate).toHaveBeenCalledTimes(2);
  });

  it('propagates upstream errors rather than reporting fabricated healthy data', async () => {
    simulate.mockRejectedValueOnce(new Error('RPC unavailable'));
    expect((await request(app).get(route)).status).toBe(500);
    expect((await request(app).get(route)).status).toBe(200);
    expect(simulate).toHaveBeenCalledTimes(2);
  });
});
