import assert from 'node:assert/strict';
import test from 'node:test';

// These tests exercise the compiled JavaScript so they need no TS loader.
// Build once with `npx tsc backend/src/services/gracePeriodAlerts.ts
//   --module nodenext --target es2022 --outDir backend/dist` before running.
import {
  GracePeriodAlertService,
  classifyHealth,
} from '../dist/gracePeriodAlerts.js';

const thresholds = { warningHf: 1.5, graceHf: 1.2, liquidationHf: 1.0 };

function position(healthFactor, overrides = {}) {
  return {
    address: 'GUSER',
    asset: 'XLM',
    healthFactor,
    collateralValue: 20_000,
    debtValue: 10_000,
    ...overrides,
  };
}

function notifier() {
  return {
    sent: [],
    async send(alert) {
      this.sent.push(alert);
      return { delivered: true, channel: 'push' };
    },
  };
}

test('classification covers the grace funnel', () => {
  assert.equal(classifyHealth(position(1.8), thresholds), 'safe');
  assert.equal(classifyHealth(position(1.3), thresholds), 'warning');
  assert.equal(classifyHealth(position(1.1), thresholds), 'grace');
  assert.equal(classifyHealth(position(0.9), thresholds), 'critical');
});

test('escalation bypasses cooldown but repeats are suppressed', async () => {
  let now = 0;
  const delivery = notifier();
  const service = new GracePeriodAlertService(delivery, {
    thresholds,
    cooldownMs: 100,
    now: () => now,
  });

  assert.equal((await service.process(position(1.3))).reason, 'alert_sent');
  assert.equal((await service.process(position(1.3))).reason, 'cooldown');

  now = 101;
  assert.equal((await service.process(position(1.3))).reason, 'alert_sent');
  assert.equal((await service.process(position(1.1))).reason, 'alert_sent');
  assert.equal(delivery.sent.at(-1).stage, 'grace');
});

test('safe positions clear previous state and alerts cap repeats', async () => {
  let now = 0;
  const delivery = notifier();
  const service = new GracePeriodAlertService(delivery, {
    thresholds,
    cooldownMs: 0,
    maxAlertsPerPosition: 2,
    now: () => now,
  });

  await service.process(position(1.1));
  await service.process(position(1.1));
  assert.equal((await service.process(position(1.1))).reason, 'max_reached');
  assert.equal((await service.process(position(2))).reason, 'safe');
  assert.equal((await service.process(position(1.1))).reason, 'alert_sent');
});

test('failed delivery is retried on the next scan', async () => {
  const delivery = notifier();
  delivery.send = async () => ({ delivered: false, error: 'offline' });
  const service = new GracePeriodAlertService(delivery, { thresholds });

  assert.equal((await service.process(position(1.1))).reason, 'delivery_failed');
  assert.equal(service.getState('GUSER', 'XLM'), undefined);
});
