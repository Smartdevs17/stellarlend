import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import {
  createKeeperReviewService,
  KeeperReviewError,
  type KeeperSubmission,
  type ReviewLogger,
  type ReviewMetrics,
  type SlashOutcome,
} from './keeperReview.ts';

function submission(id = 'sub-1'): KeeperSubmission {
  return {
    id,
    keeper: 'G_KEEPER',
    payloadHash: '0x' + 'ab'.repeat(32),
    submittedAt: 1_700_000_000,
  };
}

function harness(outcome: SlashOutcome = { slashed: true, amount: 250, txHash: 'tx-1' }) {
  const calls: Array<{ submissionId: string; keeper: string; reason: string }> = [];
  const events: Array<{ event: string; fields?: Record<string, unknown> }> = [];
  const metrics: Array<{ name: string; labels?: Record<string, string | number> }> = [];
  let fail = false;

  const logger: ReviewLogger = {
    info: (event, fields) => events.push({ event, fields }),
    warn: (event, fields) => events.push({ event, fields }),
    error: (event, fields) => events.push({ event, fields }),
  };
  const meter: ReviewMetrics = {
    increment: (name, labels) => metrics.push({ name, labels }),
  };
  const service = createKeeperReviewService({
    reviewers: ['G_REVIEWER', 'G_ADMIN'],
    slashClient: {
      async slash(request) {
        calls.push(request);
        if (fail) throw new Error('rpc unavailable');
        return outcome;
      },
    },
    logger,
    metrics: meter,
  });

  return {
    service,
    calls,
    events,
    metrics,
    setFail: (value: boolean) => { fail = value; },
  };
}

describe('keeper review service', () => {
  it('accepts a submission and records an audit log and metric', async () => {
    const h = harness();
    const result = await h.service.review(submission(), 'accept', 'G_REVIEWER');

    assert.equal(result.slashed, false);
    assert.equal(result.amount, 0);
    assert.equal(h.service.isReviewed('sub-1'), true);
    assert.equal(h.calls.length, 0);
    assert.ok(h.events.some((entry) => entry.event === 'keeper.review.accepted'));
    assert.ok(h.metrics.some((entry) => entry.name === 'keeperReviewsTotal'));
  });

  it('slashes a misbehaving keeper with a validated reason and records observability data', async () => {
    const h = harness();
    const result = await h.service.review(submission(), 'slash', 'G_ADMIN', '  conflicting attestation  ');

    assert.equal(result.slashed, true);
    assert.equal(result.amount, 250);
    assert.equal(result.txHash, 'tx-1');
    assert.deepEqual(h.calls, [{
      submissionId: 'sub-1',
      keeper: 'G_KEEPER',
      reason: 'conflicting attestation',
    }]);
    assert.ok(h.events.some((entry) => entry.event === 'keeper.review.slash_completed'));
    assert.ok(h.metrics.some((entry) => entry.name === 'keeperSlashTotal'));
  });

  it('rejects an unauthorized reviewer without marking the submission', async () => {
    const h = harness();
    await assert.rejects(
      () => h.service.review(submission(), 'slash', 'G_INTRUDER', 'double sign'),
      (error: unknown) => error instanceof KeeperReviewError && error.code === 'UNAUTHORIZED_REVIEWER',
    );
    assert.equal(h.calls.length, 0);
    assert.equal(h.service.isReviewed('sub-1'), false);
  });

  it('rejects malformed submissions, decisions and reasons explicitly', async () => {
    const h = harness();
    await assert.rejects(
      () => h.service.review({ ...submission(), keeper: ' ' }, 'slash', 'G_REVIEWER', 'double sign'),
      (error: unknown) => error instanceof KeeperReviewError && error.code === 'INVALID_SUBMISSION',
    );
    await assert.rejects(
      () => h.service.review(submission(), 'maybe' as 'accept', 'G_REVIEWER'),
      (error: unknown) => error instanceof KeeperReviewError && error.code === 'INVALID_DECISION',
    );
    await assert.rejects(
      () => h.service.review(submission(), 'slash', 'G_REVIEWER', '  '),
      (error: unknown) => error instanceof KeeperReviewError && error.code === 'INVALID_REASON',
    );
    assert.equal(h.calls.length, 0);
    assert.equal(h.service.reviewedCount, 0);
  });

  it('is idempotent: an already reviewed submission cannot be slashed again', async () => {
    const h = harness();
    await h.service.review(submission(), 'accept', 'G_REVIEWER');
    await assert.rejects(
      () => h.service.review(submission(), 'slash', 'G_REVIEWER', 'double sign'),
      (error: unknown) => error instanceof KeeperReviewError && error.code === 'ALREADY_REVIEWED',
    );
    assert.equal(h.calls.length, 0);
  });

  it('a failed slash is actionable and leaves no partial state', async () => {
    const h = harness();
    h.setFail(true);
    await assert.rejects(
      () => h.service.review(submission(), 'slash', 'G_REVIEWER', 'double sign'),
      (error: unknown) => error instanceof KeeperReviewError && error.code === 'SLASH_FAILED',
    );
    assert.equal(h.service.isReviewed('sub-1'), false);
    assert.ok(h.events.some((entry) => entry.event === 'keeper.review.slash_failed'));
    assert.ok(h.metrics.some((entry) => entry.name === 'keeperSlashFailedTotal'));

    h.setFail(false);
    const result = await h.service.review(submission(), 'slash', 'G_REVIEWER', 'double sign');
    assert.equal(result.slashed, true);
    assert.equal(h.calls.length, 2);
  });

  it('rejects an invalid slashing-client outcome without marking the submission', async () => {
    const h = harness({ slashed: true, amount: Number.NaN });
    await assert.rejects(
      () => h.service.review(submission(), 'slash', 'G_REVIEWER', 'double sign'),
      (error: unknown) => error instanceof KeeperReviewError && error.code === 'INVALID_SLASH_OUTCOME',
    );
    assert.equal(h.service.isReviewed('sub-1'), false);
  });

  it('requires a configured reviewer and slashing client', () => {
    assert.throws(
      () => createKeeperReviewService({ reviewers: [], slashClient: { slash: async () => ({ slashed: true, amount: 1 }) } }),
      (error: unknown) => error instanceof KeeperReviewError && error.code === 'UNAUTHORIZED_REVIEWER',
    );
  });
});
