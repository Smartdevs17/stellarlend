import request from 'supertest';
import {
  Account,
  Address,
  Contract,
  Keypair,
  SorobanDataBuilder,
  StrKey,
  TransactionBuilder,
  nativeToScVal,
  xdr,
} from '@stellar/stellar-sdk';
import app from '../app';
import { config } from '../config';
import { readCacheService } from '../services/readCache.service';
import { redisCacheService } from '../services/redisCache.service';

const mockSimulate = jest.fn();
jest.mock('@stellar/stellar-sdk/rpc', () => ({
  Server: jest.fn().mockImplementation(() => ({
    // Resolved lazily: the factory is hoisted above the mockSimulate declaration.
    simulateTransaction: (...args: unknown[]) => mockSimulate(...args),
  })),
}));

const USER = Keypair.random().publicKey();
const CONTRACT_ID = StrKey.encodeContract(Buffer.alloc(32, 3));

function envelope() {
  return new TransactionBuilder(new Account(USER, '5'), {
    fee: '100',
    networkPassphrase: config.stellar.networkPassphrase,
  })
    .addOperation(
      new Contract(CONTRACT_ID).call(
        'borrow_asset',
        new Address(USER).toScVal(),
        xdr.ScVal.scvVoid(),
        nativeToScVal(BigInt(1000), { type: 'i128' })
      )
    )
    .setTimeout(300)
    .build()
    .toXDR();
}

function success() {
  return {
    latestLedger: 100,
    events: [],
    transactionData: new SorobanDataBuilder().setResources(1000, 10, 20).setResourceFee(900),
    minResourceFee: '900',
    result: { auth: [], retval: nativeToScVal(true) },
  };
}

describe('Transaction simulation and fee estimation routes', () => {
  beforeEach(() => {
    mockSimulate.mockReset();
    redisCacheService.clearAllForTests();
    readCacheService.resetForTests();
  });

  describe('POST /api/lending/simulate', () => {
    it('returns the normalized simulation for an envelope', async () => {
      mockSimulate.mockResolvedValue(success());
      const res = await request(app)
        .post('/api/lending/simulate')
        .send({ transactionXdr: envelope() });

      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({
        success: true,
        status: 'success',
        sourceAccount: USER,
        operationCount: 1,
        latestLedger: 100,
        minResourceFee: '900',
        resources: {
          cpuInstructions: '1000',
          readBytes: '10',
          writeBytes: '20',
          resourceFee: '900',
        },
        result: { retval: true, auth: [] },
        cached: false,
      });
    });

    it('is also available under the versioned lending prefix', async () => {
      mockSimulate.mockResolvedValue(success());
      const res = await request(app)
        .post('/api/v1/lending/simulate')
        .send({ transactionXdr: envelope() });
      expect(res.status).toBe(200);
      expect(res.body.status).toBe('success');
    });

    it('reports failed simulations with status error and 200', async () => {
      mockSimulate.mockResolvedValue({
        latestLedger: 7,
        error: 'HostError: Error(Contract, #12)',
        events: ['AAA'],
      });
      const res = await request(app)
        .post('/api/lending/simulate')
        .send({ transactionXdr: envelope() });
      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({
        success: false,
        status: 'error',
        error: 'HostError: Error(Contract, #12)',
        events: ['AAA'],
      });
    });

    it('rejects invalid bodies with 400', async () => {
      const res = await request(app)
        .post('/api/lending/simulate')
        .send({ operation: 'deposit', amount: '1' });
      expect(res.status).toBe(400);
      expect(res.body.error.message).toContain('userAddress');
      expect(mockSimulate).not.toHaveBeenCalled();
    });

    it('rejects an unparsable envelope with 400', async () => {
      const res = await request(app).post('/api/lending/simulate').send({ transactionXdr: 'nope' });
      expect(res.status).toBe(400);
    });

    it('returns 502 when Soroban RPC is unreachable', async () => {
      mockSimulate.mockRejectedValue(new Error('socket hang up'));
      const res = await request(app)
        .post('/api/lending/simulate')
        .send({ transactionXdr: envelope() });
      expect(res.status).toBe(502);
      expect(res.body.error.code).toBe('NETWORK_ERROR');
    });
  });

  describe('POST /api/gas/estimate-transaction', () => {
    it('returns fee components derived from one simulation', async () => {
      mockSimulate.mockResolvedValue(success());
      const res = await request(app)
        .post('/api/gas/estimate-transaction')
        .send({ transactionXdr: envelope(), feeMarginPercent: 20 });

      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({
        success: true,
        status: 'success',
        operationCount: 1,
        restoreRequired: false,
        fees: {
          inclusionFee: '100',
          resourceFee: '900',
          totalFee: '1000',
          feeMarginPercent: 20,
          recommendedFee: '1200',
        },
        resources: { cpuInstructions: '1000' },
      });
      expect(mockSimulate).toHaveBeenCalledTimes(1);
    });

    it('reuses the cached simulation for repeated estimates of the same envelope', async () => {
      mockSimulate.mockResolvedValue(success());
      const xdrEnvelope = envelope();
      await request(app)
        .post('/api/gas/estimate-transaction')
        .send({ transactionXdr: xdrEnvelope });
      const res = await request(app)
        .post('/api/gas/estimate-transaction')
        .send({ transactionXdr: xdrEnvelope });
      expect(res.body.cached).toBe(true);
      expect(mockSimulate).toHaveBeenCalledTimes(1);
    });

    it('is also available under the versioned gas prefix', async () => {
      mockSimulate.mockResolvedValue(success());
      const res = await request(app)
        .post('/api/v1/lending/gas/estimate-transaction')
        .send({ transactionXdr: envelope() });
      expect(res.status).toBe(200);
      expect(res.body.fees.recommendedFee).toBe('1100');
    });

    it('returns 422 when the simulation fails', async () => {
      mockSimulate.mockResolvedValue({
        latestLedger: 7,
        error: 'HostError: Error(Contract, #12)',
        events: [],
      });
      const res = await request(app)
        .post('/api/gas/estimate-transaction')
        .send({ transactionXdr: envelope() });
      expect(res.status).toBe(422);
      expect(res.body.error).toMatchObject({
        code: 'CONTRACT_ERROR',
        details: { simulationError: 'HostError: Error(Contract, #12)', latestLedger: 7 },
      });
    });

    it('rejects an out-of-range fee margin with 400', async () => {
      const res = await request(app)
        .post('/api/gas/estimate-transaction')
        .send({ transactionXdr: envelope(), feeMarginPercent: 250 });
      expect(res.status).toBe(400);
      expect(mockSimulate).not.toHaveBeenCalled();
    });
  });
});
