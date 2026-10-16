import request from 'supertest';
import app from '../app';
import { resetReinvestmentStore } from '../services/earnings/reinvestment.service';
import { generateToken } from '../middleware/auth';

const USER = 'GA6T6URCJEEWVTUFCFBP3OONDUTFOSAFUQDIITIUU2PYNTS4YEQKGP5E';
const OTHER_USER = 'GCTC7JUZWBLSTM5N43G3EO2OE53NAIAAU7OKRQTS5XVNLWZVBVKCK5AV';
const POOL_A = 'GBNNVJG4O3HMCGM5C4ORI4O4H3K5CQA6OTKIW2KKWY2EEG2IL3TRXK32';
const VALID_TX_HASH = 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2';

describe('Reinvestment routes', () => {
  let userToken: string;
  let otherToken: string;

  beforeEach(() => {
    resetReinvestmentStore();
    userToken = generateToken(USER);
    otherToken = generateToken(OTHER_USER);
  });

  it('supports the full create -> pause -> resume -> sweep -> history -> analytics flow with authentication', async () => {
    const createResponse = await request(app)
      .post('/api/reinvestment/plan')
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        userAddress: USER,
        sourcePool: POOL_A,
        strategy: 'same_pool',
        schedule: 'real_time',
        thresholdAmount: '100',
      });
    expect(createResponse.status).toBe(201);
    expect(createResponse.body.success).toBe(true);
    const planId = createResponse.body.plan.id;
    expect(planId).toBeTruthy();

    const getResponse = await request(app).get(`/api/reinvestment/plan/${planId}`);
    expect(getResponse.status).toBe(200);
    expect(getResponse.body.plan.paused).toBe(false);

    const listResponse = await request(app).get(`/api/reinvestment/plans/${USER}`);
    expect(listResponse.status).toBe(200);
    expect(listResponse.body.plans).toHaveLength(1);

    const pauseResponse = await request(app)
      .post(`/api/reinvestment/plan/${planId}/pause`)
      .set('Authorization', `Bearer ${userToken}`)
      .send({ userAddress: USER });
    expect(pauseResponse.status).toBe(200);
    expect(pauseResponse.body.plan.paused).toBe(true);

    const blockedSweep = await request(app)
      .post(`/api/reinvestment/plan/${planId}/sweep`)
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        earnedAmount: '500',
        estimatedGasCost: '1',
        poolPaused: false,
        txHash: VALID_TX_HASH,
      });
    expect(blockedSweep.status).toBe(409);
    expect(blockedSweep.body.success).toBe(false);

    const resumeResponse = await request(app)
      .post(`/api/reinvestment/plan/${planId}/resume`)
      .set('Authorization', `Bearer ${userToken}`)
      .send({ userAddress: USER });
    expect(resumeResponse.status).toBe(200);
    expect(resumeResponse.body.plan.paused).toBe(false);

    const sweepResponse = await request(app)
      .post(`/api/reinvestment/plan/${planId}/sweep`)
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        earnedAmount: '500',
        estimatedGasCost: '1',
        poolPaused: false,
        txHash: VALID_TX_HASH,
      });
    expect(sweepResponse.status).toBe(201);
    expect(sweepResponse.body.events).toHaveLength(1);
    expect(sweepResponse.body.events[0].pool).toBe(POOL_A);

    const historyResponse = await request(app).get(`/api/reinvestment/plan/${planId}/history`);
    expect(historyResponse.status).toBe(200);
    expect(historyResponse.body.history).toHaveLength(1);

    const analyticsResponse = await request(app).get(`/api/reinvestment/plan/${planId}/analytics`);
    expect(analyticsResponse.status).toBe(200);
    expect(analyticsResponse.body.analytics.totalSweeps).toBe(1);
  });

  it('rejects unauthenticated plan creation, pause, resume, and sweep with 401', async () => {
    const unauthCreate = await request(app).post('/api/reinvestment/plan').send({
      userAddress: USER,
      sourcePool: POOL_A,
      strategy: 'same_pool',
      schedule: 'real_time',
      thresholdAmount: '100',
    });
    expect(unauthCreate.status).toBe(401);

    // Create a legitimate plan with auth
    const createResponse = await request(app)
      .post('/api/reinvestment/plan')
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        userAddress: USER,
        sourcePool: POOL_A,
        strategy: 'same_pool',
        schedule: 'real_time',
        thresholdAmount: '100',
      });
    const planId = createResponse.body.plan.id;

    const unauthPause = await request(app)
      .post(`/api/reinvestment/plan/${planId}/pause`)
      .send({ userAddress: USER });
    expect(unauthPause.status).toBe(401);

    const unauthResume = await request(app)
      .post(`/api/reinvestment/plan/${planId}/resume`)
      .send({ userAddress: USER });
    expect(unauthResume.status).toBe(401);

    const unauthSweep = await request(app)
      .post(`/api/reinvestment/plan/${planId}/sweep`)
      .send({
        earnedAmount: '500',
        estimatedGasCost: '1',
        poolPaused: false,
        txHash: VALID_TX_HASH,
      });
    expect(unauthSweep.status).toBe(401);
  });

  it('prevents cross-user manipulation: another user cannot pause, resume, or sweep victim plan', async () => {
    const createResponse = await request(app)
      .post('/api/reinvestment/plan')
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        userAddress: USER,
        sourcePool: POOL_A,
        strategy: 'same_pool',
        schedule: 'real_time',
        thresholdAmount: '100',
      });
    const planId = createResponse.body.plan.id;

    // Attacker attempts to pause victim's plan
    const crossPause = await request(app)
      .post(`/api/reinvestment/plan/${planId}/pause`)
      .set('Authorization', `Bearer ${otherToken}`)
      .send({ userAddress: USER });
    expect(crossPause.status).toBe(401);

    // Attacker attempts to sweep victim's plan
    const crossSweep = await request(app)
      .post(`/api/reinvestment/plan/${planId}/sweep`)
      .set('Authorization', `Bearer ${otherToken}`)
      .send({
        earnedAmount: '500',
        estimatedGasCost: '1',
        poolPaused: false,
        txHash: VALID_TX_HASH,
      });
    expect(crossSweep.status).toBe(401);
  });

  it('rejects sweep without required valid txHash with 400', async () => {
    const createResponse = await request(app)
      .post('/api/reinvestment/plan')
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        userAddress: USER,
        sourcePool: POOL_A,
        strategy: 'same_pool',
        schedule: 'real_time',
        thresholdAmount: '100',
      });
    const planId = createResponse.body.plan.id;

    const noTxHashResponse = await request(app)
      .post(`/api/reinvestment/plan/${planId}/sweep`)
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        earnedAmount: '500',
        estimatedGasCost: '1',
        poolPaused: false,
      });
    expect(noTxHashResponse.status).toBe(400);

    const invalidTxHashResponse = await request(app)
      .post(`/api/reinvestment/plan/${planId}/sweep`)
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        earnedAmount: '500',
        estimatedGasCost: '1',
        poolPaused: false,
        txHash: 'short-hash',
      });
    expect(invalidTxHashResponse.status).toBe(400);
  });

  it('rejects plan creation with an invalid strategy', async () => {
    const response = await request(app)
      .post('/api/reinvestment/plan')
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        userAddress: USER,
        sourcePool: POOL_A,
        strategy: 'not_a_real_strategy',
        schedule: 'real_time',
        thresholdAmount: '0',
      });
    expect(response.status).toBe(400);
    expect(response.body.success).toBe(false);
  });

  it('returns 404 for an unknown plan id', async () => {
    const response = await request(app).get('/api/reinvestment/plan/does-not-exist');
    expect(response.status).toBe(404);
    expect(response.body.success).toBe(false);
  });

  it('rejects a sweep below the configured threshold', async () => {
    const createResponse = await request(app)
      .post('/api/reinvestment/plan')
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        userAddress: USER,
        sourcePool: POOL_A,
        strategy: 'same_pool',
        schedule: 'real_time',
        thresholdAmount: '1000',
      });
    const planId = createResponse.body.plan.id;

    const sweepResponse = await request(app)
      .post(`/api/reinvestment/plan/${planId}/sweep`)
      .set('Authorization', `Bearer ${userToken}`)
      .send({
        earnedAmount: '10',
        estimatedGasCost: '1',
        poolPaused: false,
        txHash: VALID_TX_HASH,
      });
    expect(sweepResponse.status).toBe(400);
    expect(sweepResponse.body.success).toBe(false);
  });
});
