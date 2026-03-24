// Mock StellarService before importing app
import { StellarService } from '../services/stellar.service';
jest.mock('../services/stellar.service');

// Robust global Axios mock to prevent real network calls
import axios from 'axios';
jest.mock('axios');
const mockedAxios = axios as jest.Mocked<typeof axios>;

import request from 'supertest';
import app from '../app';

const VALID_ADDRESS = 'GDZZJ3UPZZCKY5DBH6ZGMPMRORRBG4ECIORASBUAXPPNCL4SYRHNLYU2';
const VALID_AMOUNT = '10000000';

const mockStellarService: jest.Mocked<StellarService> = {
  buildUnsignedTransaction: jest.fn(),
  submitTransaction: jest.fn(),
  monitorTransaction: jest.fn(),
  healthCheck: jest.fn(),
} as any;

beforeAll(() => {
  (StellarService as jest.Mock).mockImplementation(() => mockStellarService);

  mockedAxios.create.mockReturnThis();
  const axiosResponse = {
    data: {},
    status: 200,
    statusText: 'OK',
    headers: {},
    config: { url: '' },
  };
  mockedAxios.get.mockResolvedValue(axiosResponse);
  mockedAxios.post.mockResolvedValue(axiosResponse);
  mockedAxios.request.mockResolvedValue(axiosResponse);
});

beforeEach(() => {
  jest.clearAllMocks();
  // Default happy-path mock responses
  mockStellarService.buildUnsignedTransaction.mockResolvedValue('unsigned_xdr_string');
  mockStellarService.submitTransaction.mockResolvedValue({
    success: true,
    transactionHash: 'abc123txhash',
    status: 'success',
  });
  mockStellarService.monitorTransaction.mockResolvedValue({
    success: true,
    transactionHash: 'abc123txhash',
    status: 'success',
    ledger: 12345,
  });
  mockStellarService.healthCheck.mockResolvedValue({ horizon: true, sorobanRpc: true });
});

// ─── 1. Complete Deposit Flow ─────────────────────────────────────────────────

describe('Complete Deposit Flow', () => {
  it('prepare returns unsigned XDR with correct shape', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT });

    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      unsignedXdr: 'unsigned_xdr_string',
      operation: 'deposit',
    });
    expect(typeof res.body.expiresAt).toBe('string');
    expect(new Date(res.body.expiresAt).getTime()).toBeGreaterThan(Date.now());
  });

  it('prepare calls buildUnsignedTransaction with correct args', async () => {
    await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT });

    expect(mockStellarService.buildUnsignedTransaction).toHaveBeenCalledTimes(1);
    expect(mockStellarService.buildUnsignedTransaction).toHaveBeenCalledWith(
      'deposit',
      VALID_ADDRESS,
      undefined,
      VALID_AMOUNT
    );
  });

  it('submit returns success with transaction hash and ledger', async () => {
    const res = await request(app)
      .post('/api/lending/submit')
      .send({ signedXdr: 'signed_xdr_payload' });

    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      success: true,
      transactionHash: 'abc123txhash',
      status: 'success',
      ledger: 12345,
    });
  });

  it('submit calls monitorTransaction after successful submitTransaction', async () => {
    await request(app)
      .post('/api/lending/submit')
      .send({ signedXdr: 'signed_xdr_payload' });

    expect(mockStellarService.submitTransaction).toHaveBeenCalledWith('signed_xdr_payload');
    expect(mockStellarService.monitorTransaction).toHaveBeenCalledWith('abc123txhash');
  });

  it('full prepare → submit lifecycle returns consistent data', async () => {
    const prepareRes = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT });

    expect(prepareRes.status).toBe(200);
    expect(prepareRes.body.unsignedXdr).toBe('unsigned_xdr_string');

    const submitRes = await request(app)
      .post('/api/lending/submit')
      .send({ signedXdr: 'client_signed_xdr' });

    expect(submitRes.status).toBe(200);
    expect(submitRes.body.success).toBe(true);
    expect(submitRes.body.transactionHash).toBe('abc123txhash');
  });
});

// ─── 2. Error Handling ────────────────────────────────────────────────────────

describe('Error Handling', () => {
  it('returns 400 for an invalid operation name', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/invalid_op')
      .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT });

    expect(res.status).toBe(400);
    expect(res.body).toHaveProperty('error');
  });

  it('returns 400 when userAddress is missing', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ amount: VALID_AMOUNT });

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/address/i);
  });

  it('returns 400 when amount is missing', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS });

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/amount/i);
  });

  it('returns 400 when userAddress is not a valid Stellar key', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: 'NOT_A_STELLAR_ADDRESS', amount: VALID_AMOUNT });

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/stellar address/i);
  });

  it('returns 400 when signedXdr is missing on submit', async () => {
    const res = await request(app).post('/api/lending/submit').send({});

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/signedXdr/i);
  });

  it('returns 400 when submit receives malformed JSON', async () => {
    const res = await request(app)
      .post('/api/lending/submit')
      .set('Content-Type', 'application/json')
      .send('{ bad json }');

    expect(res.status).toBe(400);
  });

  it('returns 500 when stellar service fails to build transaction', async () => {
    mockStellarService.buildUnsignedTransaction.mockRejectedValueOnce(
      new Error('Stellar network error')
    );

    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT });

    expect(res.status).toBe(500);
    expect(res.body).toHaveProperty('error');
  });

  it('returns 400 from submit when submitTransaction reports failure', async () => {
    mockStellarService.submitTransaction.mockResolvedValueOnce({
      success: false,
      status: 'failed',
      error: 'tx_bad_seq',
    });

    const res = await request(app)
      .post('/api/lending/submit')
      .send({ signedXdr: 'bad_xdr' });

    expect(res.status).toBe(400);
    expect(res.body.success).toBe(false);
    expect(res.body.error).toBe('tx_bad_seq');
  });

  it('health endpoint returns 503 when services are down', async () => {
    mockStellarService.healthCheck.mockResolvedValueOnce({
      horizon: false,
      sorobanRpc: false,
    });

    const res = await request(app).get('/api/health');

    expect(res.status).toBe(503);
    expect(res.body.status).toBe('unhealthy');
    expect(res.body.services.horizon).toBe(false);
    expect(res.body.services.sorobanRpc).toBe(false);
  });
});

// ─── 3. Edge Cases ────────────────────────────────────────────────────────────

describe('Edge Cases', () => {
  it('rejects amount of zero', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS, amount: '0' });

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/amount/i);
  });

  it('rejects negative amount', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS, amount: '-500' });

    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/amount/i);
  });

  it('accepts optional assetAddress when provided', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT, assetAddress: VALID_ADDRESS });

    expect(res.status).toBe(200);
    expect(mockStellarService.buildUnsignedTransaction).toHaveBeenCalledWith(
      'deposit',
      VALID_ADDRESS,
      VALID_ADDRESS,
      VALID_AMOUNT
    );
  });

  it('works without optional assetAddress', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT });

    expect(res.status).toBe(200);
    expect(mockStellarService.buildUnsignedTransaction).toHaveBeenCalledWith(
      'deposit',
      VALID_ADDRESS,
      undefined,
      VALID_AMOUNT
    );
  });

  it('all four valid operations are accepted by prepare', async () => {
    for (const op of ['deposit', 'borrow', 'repay', 'withdraw']) {
      const res = await request(app)
        .get(`/api/lending/prepare/${op}`)
        .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT });

      expect(res.status).toBe(200);
      expect(res.body.operation).toBe(op);
    }
  });
});

// ─── 4. Security Headers ──────────────────────────────────────────────────────

describe('Security Headers', () => {
  it('includes x-content-type-options header', async () => {
    const res = await request(app).get('/api/health');
    expect(res.headers['x-content-type-options']).toBe('nosniff');
  });

  it('includes x-frame-options header', async () => {
    const res = await request(app).get('/api/health');
    expect(res.headers['x-frame-options']).toBeDefined();
  });

  it('includes strict-transport-security header', async () => {
    const res = await request(app).get('/api/health');
    expect(res.headers['strict-transport-security']).toMatch(/max-age/);
  });

  it('responds to OPTIONS preflight requests', async () => {
    const res = await request(app).options('/api/lending/prepare/deposit');
    expect([200, 204]).toContain(res.status);
  });

  it('health endpoint returns healthy status with correct shape', async () => {
    const res = await request(app).get('/api/health');

    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      status: 'healthy',
      services: { horizon: true, sorobanRpc: true },
    });
    expect(typeof res.body.timestamp).toBe('string');
  });
});

// ─── 5. Concurrent Requests & Rate Limiting ───────────────────────────────────
// NOTE: This suite fires 110 requests and exhausts the rate limit window.
// It is placed last so the burst does not affect other test suites.

describe('Concurrent Requests', () => {
  it('handles multiple simultaneous prepare requests independently', async () => {
    const operations: Array<'deposit' | 'borrow' | 'repay' | 'withdraw'> = [
      'deposit',
      'borrow',
      'repay',
      'withdraw',
    ];

    const responses = await Promise.all(
      operations.map((op) =>
        request(app)
          .get(`/api/lending/prepare/${op}`)
          .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT })
      )
    );

    responses.forEach((res, i) => {
      expect(res.status).toBe(200);
      expect(res.body.operation).toBe(operations[i]);
      expect(res.body.unsignedXdr).toBe('unsigned_xdr_string');
    });
  });

  it('each concurrent request gets its own response body', async () => {
    mockStellarService.buildUnsignedTransaction
      .mockResolvedValueOnce('xdr_for_user_1')
      .mockResolvedValueOnce('xdr_for_user_2');

    const [res1, res2] = await Promise.all([
      request(app)
        .get('/api/lending/prepare/deposit')
        .query({ userAddress: VALID_ADDRESS, amount: '1000000' }),
      request(app)
        .get('/api/lending/prepare/deposit')
        .query({ userAddress: VALID_ADDRESS, amount: '2000000' }),
    ]);

    expect(res1.status).toBe(200);
    expect(res2.status).toBe(200);
    expect(res1.body.unsignedXdr).toBe('xdr_for_user_1');
    expect(res2.body.unsignedXdr).toBe('xdr_for_user_2');
  });

  it('rate limiter returns 429 after exceeding the configured limit', async () => {
    // Fire 110 requests to exceed the 100 req/window default limit
    const total = 110;
    const responses = await Promise.all(
      Array.from({ length: total }, () =>
        request(app)
          .get('/api/lending/prepare/deposit')
          .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT })
      )
    );

    const statuses = responses.map((r) => r.status);
    expect(statuses).toContain(200);
    expect(statuses).toContain(429);
  });
});
