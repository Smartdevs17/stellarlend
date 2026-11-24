import request from 'supertest';
import app from '../app';
import { generateToken } from '../middleware/auth';
import { resetPoolWhitelistStore } from '../routes/pools';

const COUNCIL_A = 'GCOUNCILMEMBERAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';
const COUNCIL_B = 'GCOUNCILMEMBERBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB';
const COUNCIL_C = 'GCOUNCILMEMBERCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC';
const OUTSIDER = 'GOUTSIDERUSERAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';

const POOL = `C${'A'.repeat(55)}`;

describe('Pool whitelist routes (risk-council quorum)', () => {
  let tokenA: string;
  let tokenB: string;
  let outsiderToken: string;

  beforeEach(() => {
    resetPoolWhitelistStore();
    process.env.RISK_COUNCIL_MEMBERS = [COUNCIL_A, COUNCIL_B, COUNCIL_C].join(',');
    process.env.RISK_COUNCIL_QUORUM = '2';
    tokenA = generateToken(COUNCIL_A);
    tokenB = generateToken(COUNCIL_B);
    outsiderToken = generateToken(OUTSIDER);
  });

  afterEach(() => {
    delete process.env.RISK_COUNCIL_MEMBERS;
    delete process.env.RISK_COUNCIL_QUORUM;
  });

  it('requires authentication to propose or approve', async () => {
    const propose = await request(app).post('/api/pools/whitelist/proposals').send({ pool: POOL });
    expect(propose.status).toBe(401);

    const approve = await request(app).post('/api/pools/whitelist/proposals/missing/approvals');
    expect(approve.status).toBe(401);
  });

  it('rejects proposals and approvals from addresses outside the council', async () => {
    const propose = await request(app)
      .post('/api/pools/whitelist/proposals')
      .set('Authorization', `Bearer ${outsiderToken}`)
      .send({ pool: POOL });
    expect(propose.status).toBe(403);

    const created = await request(app)
      .post('/api/pools/whitelist/proposals')
      .set('Authorization', `Bearer ${tokenA}`)
      .send({ pool: POOL });
    const approve = await request(app)
      .post(`/api/pools/whitelist/proposals/${created.body.data.id}/approvals`)
      .set('Authorization', `Bearer ${outsiderToken}`);
    expect(approve.status).toBe(403);
  });

  it('rejects pool values that are not Stellar contract addresses', async () => {
    const res = await request(app)
      .post('/api/pools/whitelist/proposals')
      .set('Authorization', `Bearer ${tokenA}`)
      .send({ pool: 'not-a-contract' });
    expect(res.status).toBe(400);
  });

  it('keeps a pool pending until a quorum of distinct members approves it', async () => {
    const created = await request(app)
      .post('/api/pools/whitelist/proposals')
      .set('Authorization', `Bearer ${tokenA}`)
      .send({ pool: POOL });
    expect(created.status).toBe(201);
    expect(created.body.data).toMatchObject({
      pool: POOL,
      status: 'pending',
      quorum: 2,
      approvals: [COUNCIL_A],
    });

    const duplicateApproval = await request(app)
      .post(`/api/pools/whitelist/proposals/${created.body.data.id}/approvals`)
      .set('Authorization', `Bearer ${tokenA}`);
    expect(duplicateApproval.status).toBe(409);

    const notYetListed = await request(app).get(`/api/pools/whitelist/${POOL}`);
    expect(notYetListed.body.data.whitelisted).toBe(false);

    const approved = await request(app)
      .post(`/api/pools/whitelist/proposals/${created.body.data.id}/approvals`)
      .set('Authorization', `Bearer ${tokenB}`);
    expect(approved.status).toBe(200);
    expect(approved.body.data.status).toBe('whitelisted');

    const status = await request(app).get(`/api/pools/whitelist/${POOL}`);
    expect(status.body.data.whitelisted).toBe(true);

    const list = await request(app).get('/api/pools/whitelist');
    expect(list.body.data.map((entry: { pool: string }) => entry.pool)).toEqual([POOL]);
  });

  it('refuses a second proposal for a pool that is pending or already whitelisted', async () => {
    const first = await request(app)
      .post('/api/pools/whitelist/proposals')
      .set('Authorization', `Bearer ${tokenA}`)
      .send({ pool: POOL });
    expect(first.status).toBe(201);

    const duplicatePending = await request(app)
      .post('/api/pools/whitelist/proposals')
      .set('Authorization', `Bearer ${tokenB}`)
      .send({ pool: POOL });
    expect(duplicatePending.status).toBe(409);

    await request(app)
      .post(`/api/pools/whitelist/proposals/${first.body.data.id}/approvals`)
      .set('Authorization', `Bearer ${tokenB}`);

    const afterWhitelisting = await request(app)
      .post('/api/pools/whitelist/proposals')
      .set('Authorization', `Bearer ${tokenA}`)
      .send({ pool: POOL });
    expect(afterWhitelisting.status).toBe(409);
  });

  it('fails closed when no council is configured', async () => {
    delete process.env.RISK_COUNCIL_MEMBERS;
    const res = await request(app)
      .post('/api/pools/whitelist/proposals')
      .set('Authorization', `Bearer ${tokenA}`)
      .send({ pool: POOL });
    expect(res.status).toBe(403);
  });

  it('reports a misconfigured quorum instead of whitelisting', async () => {
    process.env.RISK_COUNCIL_QUORUM = '5';
    const res = await request(app)
      .post('/api/pools/whitelist/proposals')
      .set('Authorization', `Bearer ${tokenA}`)
      .send({ pool: POOL });
    expect(res.status).toBe(500);
    const list = await request(app).get('/api/pools/whitelist');
    expect(list.body.data).toEqual([]);
  });

  it('returns 404 for an unknown proposal', async () => {
    const res = await request(app)
      .post('/api/pools/whitelist/proposals/does-not-exist/approvals')
      .set('Authorization', `Bearer ${tokenA}`);
    expect(res.status).toBe(404);
  });
});
