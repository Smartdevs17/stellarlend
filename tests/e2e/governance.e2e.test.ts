/**
 * Governance Lifecycle E2E Tests (Issue #690)
 *
 * Exercises the full proposal lifecycle against the in-memory governance
 * harness: initialize → create → vote → queue → execute, plus cancellation,
 * defeat, quorum/threshold gates, and timelock/execution-delay enforcement.
 */

import request from 'supertest';

import {
  buildGovernanceApp,
  GovProposalStatus,
  initializeGovernance,
  reset,
} from './scenarios/harness';

const ADMIN = 'GADMIN';
const PROPOSER = 'GPROPOSER';
const VOTER1 = 'GVOTER1';
const VOTER2 = 'GVOTER2';

async function createProposal(
  app: ReturnType<typeof buildGovernanceApp>,
  description = 'Raise min collateral ratio'
): Promise<number> {
  const res = await request(app)
    .post('/api/governance/proposals')
    .send({
      proposer: PROPOSER,
      proposalType: 'MinCollateralRatio',
      description,
      votingThreshold: 5000,
    });
  expect(res.status).toBe(200);
  return res.body.proposalId as number;
}

describe('Governance lifecycle E2E', () => {
  beforeEach(() => {
    reset();
    initializeGovernance({
      votingPeriod: 100,
      executionDelay: 50,
      quorumBps: 4000,
      proposalThreshold: 100,
      timelockDuration: 200,
      defaultVotingThreshold: 5000,
      balances: { [PROPOSER]: 1000, [VOTER1]: 5000, [VOTER2]: 5000 },
    });
  });

  it('initializes governance config with expected parameters', async () => {
    const app = buildGovernanceApp();
    const res = await request(app).get('/api/governance/config');
    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      votingPeriod: 100,
      executionDelay: 50,
      quorumBps: 4000,
      proposalThreshold: 100,
      timelockDuration: 200,
      defaultVotingThreshold: 5000,
    });
  });

  it('rejects re-initialization', async () => {
    const app = buildGovernanceApp();
    const res = await request(app).post('/api/governance/initialize').send({
      admin: ADMIN,
      votingPeriod: 100,
    });
    expect(res.status).toBe(409);
  });

  it('creates a proposal in Pending with derived schedule', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    const res = await request(app).get(`/api/governance/proposals/${id}`);
    expect(res.status).toBe(200);
    expect(res.body.status).toBe(GovProposalStatus.Pending);
    expect(res.body.proposer).toBe(PROPOSER);
    expect(res.body.endTime - res.body.startTime).toBe(100);
    expect(res.body.votingThreshold).toBe(5000);
  });

  it('rejects proposals from proposers below the threshold', async () => {
    const app = buildGovernanceApp();
    const res = await request(app)
      .post('/api/governance/proposals')
      .send({
        proposer: 'GBROKE',
        proposalType: 'MinCollateralRatio',
        description: 'no tokens',
      });
    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/threshold/i);
  });

  it('activates on first vote and tallies voting power from balances', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);

    const vote = await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER1, voteType: 'For' });
    expect(vote.status).toBe(200);

    const res = await request(app).get(`/api/governance/proposals/${id}`);
    expect(res.body.status).toBe(GovProposalStatus.Active);
    expect(res.body.forVotes).toBe(5000);
    expect(res.body.totalVotingPower).toBe(5000);
  });

  it('rejects double voting', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER1, voteType: 'For' });
    const second = await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER1, voteType: 'Against' });
    expect(second.status).toBe(409);
  });

  it('rejects votes from accounts with zero balance', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    const res = await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: 'GVOTELESS', voteType: 'For' });
    expect(res.status).toBe(400);
    expect(res.body.error).toMatch(/voting power/i);
  });

  it('rejects queueing before the voting window ends', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER1, voteType: 'For' });
    const res = await request(app)
      .post(`/api/governance/proposals/${id}/queue`)
      .send({ caller: ADMIN });
    expect(res.status).toBe(409);
    expect(res.body.error).toMatch(/voting/i);
  });

  it('queues a successful proposal and sets execution time', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER1, voteType: 'For' });
    await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER2, voteType: 'For' });

    // Advance past voting period.
    await request(app).post('/api/governance/advance-time').send({ seconds: 101 });

    const queue = await request(app)
      .post(`/api/governance/proposals/${id}/queue`)
      .send({ caller: ADMIN });
    expect(queue.status).toBe(200);
    expect(queue.body.succeeded).toBe(true);
    expect(queue.body.quorumReached).toBe(true);

    const res = await request(app).get(`/api/governance/proposals/${id}`);
    expect(res.body.status).toBe(GovProposalStatus.Queued);
    expect(res.body.executionTime).toBeGreaterThan(res.body.endTime);
  });

  it('defeats a proposal that fails the voting threshold', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    // Only VOTER1 votes For (5000); VOTER2 votes Against (5000) → 50% For
    // with threshold 5000 bps of total voting power 10000 → 5000 required,
    // for_votes 5000 meets threshold… use a higher threshold instead.
    const highThresholdId = await (async () => {
      const res = await request(app)
        .post('/api/governance/proposals')
        .send({
          proposer: PROPOSER,
          proposalType: 'MinCollateralRatio',
          description: 'super majority',
          votingThreshold: 8000,
        });
      return res.body.proposalId as number;
    })();

    await request(app)
      .post(`/api/governance/proposals/${highThresholdId}/vote`)
      .send({ voter: VOTER1, voteType: 'For' });
    await request(app)
      .post(`/api/governance/proposals/${highThresholdId}/vote`)
      .send({ voter: VOTER2, voteType: 'Against' });

    await request(app).post('/api/governance/advance-time').send({ seconds: 101 });

    const queue = await request(app)
      .post(`/api/governance/proposals/${highThresholdId}/queue`)
      .send({ caller: ADMIN });
    expect(queue.status).toBe(200);
    expect(queue.body.succeeded).toBe(false);

    const res = await request(app).get(`/api/governance/proposals/${highThresholdId}`);
    expect(res.body.status).toBe(GovProposalStatus.Defeated);
    void id;
  });

  it('blocks execution before the execution delay elapses', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER1, voteType: 'For' });
    await request(app).post('/api/governance/advance-time').send({ seconds: 101 });
    await request(app)
      .post(`/api/governance/proposals/${id}/queue`)
      .send({ caller: ADMIN });

    const early = await request(app)
      .post(`/api/governance/proposals/${id}/execute`)
      .send({ executor: ADMIN });
    expect(early.status).toBe(409);
    expect(early.body.error).toMatch(/too early|delay/i);
  });

  it('executes after the delay and records side effects', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER1, voteType: 'For' });
    await request(app).post('/api/governance/advance-time').send({ seconds: 101 });
    await request(app)
      .post(`/api/governance/proposals/${id}/queue`)
      .send({ caller: ADMIN });

    // Advance past execution delay (50s).
    await request(app).post('/api/governance/advance-time').send({ seconds: 51 });

    const exec = await request(app)
      .post(`/api/governance/proposals/${id}/execute`)
      .send({ executor: ADMIN });
    expect(exec.status).toBe(200);

    const res = await request(app).get(`/api/governance/proposals/${id}`);
    expect(res.body.status).toBe(GovProposalStatus.Executed);
    expect(res.body.executedAt).toBeGreaterThan(0);
  });

  it('blocks execution after the timelock window expires', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER1, voteType: 'For' });
    await request(app).post('/api/governance/advance-time').send({ seconds: 101 });
    await request(app)
      .post(`/api/governance/proposals/${id}/queue`)
      .send({ caller: ADMIN });

    // Jump past execution_delay + timelock_duration.
    await request(app).post('/api/governance/advance-time').send({ seconds: 51 + 200 + 1 });

    const exec = await request(app)
      .post(`/api/governance/proposals/${id}/execute`)
      .send({ executor: ADMIN });
    expect(exec.status).toBe(410);

    const res = await request(app).get(`/api/governance/proposals/${id}`);
    expect(res.body.status).toBe(GovProposalStatus.Expired);
  });

  it('cancels a pending proposal by the proposer', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    const res = await request(app)
      .post(`/api/governance/proposals/${id}/cancel`)
      .send({ caller: PROPOSER });
    expect(res.status).toBe(200);

    const after = await request(app).get(`/api/governance/proposals/${id}`);
    expect(after.body.status).toBe(GovProposalStatus.Cancelled);
  });

  it('refuses to cancel an executed proposal', async () => {
    const app = buildGovernanceApp();
    const id = await createProposal(app);
    await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER1, voteType: 'For' });
    await request(app).post('/api/governance/advance-time').send({ seconds: 101 });
    await request(app)
      .post(`/api/governance/proposals/${id}/queue`)
      .send({ caller: ADMIN });
    await request(app).post('/api/governance/advance-time').send({ seconds: 51 });
    await request(app)
      .post(`/api/governance/proposals/${id}/execute`)
      .send({ executor: ADMIN });

    const res = await request(app)
      .post(`/api/governance/proposals/${id}/cancel`)
      .send({ caller: ADMIN });
    expect(res.status).toBe(409);
  });

  it('runs the happy-path lifecycle end to end', async () => {
    const app = buildGovernanceApp();

    const id = await createProposal(app, 'Full lifecycle');
    await request(app)
      .post(`/api/governance/proposals/${id}/vote`)
      .send({ voter: VOTER1, voteType: 'For' });
    await request(app).post('/api/governance/advance-time').send({ seconds: 101 });
    await request(app)
      .post(`/api/governance/proposals/${id}/queue`)
      .send({ caller: ADMIN });
    await request(app).post('/api/governance/advance-time').send({ seconds: 51 });
    await request(app)
      .post(`/api/governance/proposals/${id}/execute`)
      .send({ executor: ADMIN });

    const res = await request(app).get(`/api/governance/proposals/${id}`);
    expect(res.body.status).toBe(GovProposalStatus.Executed);
    expect(res.body.forVotes).toBe(5000);
    expect(res.body.events).toEqual(
      expect.arrayContaining(['created', 'voted', 'queued', 'executed'])
    );
  });
});
