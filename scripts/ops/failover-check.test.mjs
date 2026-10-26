import assert from 'node:assert/strict';
import test from 'node:test';

import { checkFailover } from './failover-check.mjs';

const primaryUrl = 'https://us-east.example.com/health';
const secondaryUrl = 'https://eu-west.example.com/health';

test('selects the primary when both regional endpoints are healthy', async () => {
  const result = await checkFailover({
    primaryUrl,
    secondaryUrl,
    fetchImpl: async () => new Response(null, { status: 200 }),
  });

  assert.equal(result.ready, true);
  assert.equal(result.selected.name, 'primary');
  assert.deepEqual(
    result.checks.map(({ healthy }) => healthy),
    [true, true],
  );
});

test('selects the failover endpoint when the primary is unavailable', async () => {
  const result = await checkFailover({
    primaryUrl,
    secondaryUrl,
    fetchImpl: async (url) =>
      new Response(null, { status: url.href === primaryUrl ? 503 : 200 }),
  });

  assert.equal(result.ready, false);
  assert.equal(result.selected.name, 'secondary');
  assert.equal(result.failoverSucceeded, true);
  assert.equal(result.checks[0].reason, 'HTTP 503');
});

test('reports no selected region when both endpoints fail', async () => {
  const result = await checkFailover({
    primaryUrl,
    secondaryUrl,
    fetchImpl: async () => {
      throw new TypeError('network failure');
    },
  });

  assert.equal(result.ready, false);
  assert.equal(result.selected, null);
  assert.equal(result.checks[0].reason, 'request failed (TypeError)');
});

test('rejects unsafe URLs, duplicate endpoints, and invalid timeouts', async () => {
  const options = {
    primaryUrl,
    secondaryUrl,
    fetchImpl: async () => new Response(null, { status: 200 }),
  };

  await assert.rejects(
    () => checkFailover({ ...options, primaryUrl: 'https://user:secret@example.com/health' }),
    /must not contain credentials/,
  );
  await assert.rejects(
    () => checkFailover({ ...options, secondaryUrl: primaryUrl }),
    /must be different/,
  );
  await assert.rejects(() => checkFailover({ ...options, timeoutMs: 0 }), /Timeout must be an integer/);
});
