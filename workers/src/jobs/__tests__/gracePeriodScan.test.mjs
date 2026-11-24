import assert from 'node:assert/strict';
import test from 'node:test';

// Build the two TS modules once before running:
//   npx tsc backend/src/services/gracePeriodAlerts.ts \
//     workers/src/jobs/gracePeriodScan.ts --module nodenext --target es2022 \
//     --skipLibCheck --outDir .grace-build
import { runGracePeriodScan } from '../dist/gracePeriodScan.js';

function position(healthFactor, address) {
  return {
    address,
    asset: 'XLM',
    healthFactor,
    collateralValue: 20_000,
    debtValue: 10_000,
  };
}

const metrics = {
  increments: [],
  observations: [],
  increment(name, labels) {
    this.increments.push([name, labels]);
  },
  observe(name, value) {
    this.observations.push([name, value]);
  },
};

test('scan reports alerts, skips invalid input and isolates failures', async () => {
  const source = {
    async listOpenPositions() {
      return [
        position(1.1, 'G1'),
        position(1.4, 'G2'),
        { address: '', asset: 'XLM', healthFactor: 1, collateralValue: 1, debtValue: 1 },
        position(0.9, 'G3'),
      ];
    },
  };
  const service = {
    async process(current) {
      if (current.address === 'G3') {
        throw new Error('notifier offline');
      }
      return {
        address: current.address,
        asset: current.asset,
        stage: 'grace',
        reason: 'alert_sent',
        delivered: true,
      };
    },
  };

  const report = await runGracePeriodScan(source, service, {
    now: () => 1_000,
    metrics,
  });

  assert.equal(report.scanned, 4);
  assert.equal(report.eligible, 3);
  assert.equal(report.skipped, 1);
  assert.equal(report.alertsSent, 2);
  assert.equal(report.delivered, 2);
  assert.equal(report.failed, 1);
  assert.equal(report.errors[0].address, 'G3');
  assert.equal(report.startedAt, 1_000);
  assert.equal(report.finishedAt, 1_000);
});

test('source failures produce a safe empty report', async () => {
  const source = {
    async listOpenPositions() {
      throw new Error('indexer unavailable');
    },
  };
  const service = { async process() { throw new Error('should not be called'); } };
  const report = await runGracePeriodScan(source, service, { now: () => 5_000 });
  assert.equal(report.scanned, 0);
  assert.equal(report.failed, 0);
  assert.ok(metrics.increments.some(([name]) => name === 'grace_period.scan.source_error'));
});
