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
    const { unsignedXdr } = prepareRes.body;
    expect(unsignedXdr).toBe('unsigned_xdr_string');

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
    expect(res.body).toHaveProperty('message');
  });

  it('returns 400 when userAddress is missing', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ amount: VALID_AMOUNT });

    expect(res.status).toBe(400);
    expect(res.body.message).toMatch(/address/i);
  });

  it('returns 400 when amount is missing', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS });

    expect(res.status).toBe(400);
    expect(res.body.message).toMatch(/amount/i);
  });

  it('returns 400 when userAddress is not a valid Stellar key', async () => {
    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: 'NOT_A_STELLAR_ADDRESS', amount: VALID_AMOUNT });

    expect(res.status).toBe(400);
    expect(res.body.message).toMatch(/stellar address/i);
  });

  it('returns 400 when signedXdr is missing on submit', async () => {
    const res = await request(app).post('/api/lending/submit').send({});

    expect(res.status).toBe(400);
    expect(res.body.message).toMatch(/signedXdr/i);
  });

  it('returns 400 when submit receives malformed JSON', async () => {
    const res = await request(app)
      .post('/api/lending/submit')
      .set('Content-Type', 'application/json')
      .send('{ bad json }');

    expect(res.status).toBe(400);
  });

  it('returns 400 when stellar service fails to build transaction', async () => {
    mockStellarService.buildUnsignedTransaction.mockRejectedValueOnce(
      new Error('Stellar network error')
    );

    const res = await request(app)
      .get('/api/lending/prepare/deposit')
      .query({ userAddress: VALID_ADDRESS, amount: VALID_AMOUNT });

    expect(res.status).toBe(500);
    expect(res.body).toHaveProperty('message');
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
