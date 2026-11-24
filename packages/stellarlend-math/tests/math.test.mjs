import assert from 'node:assert/strict';
import test from 'node:test';

import {
  BPS_DIVISOR,
  INDEX_SCALE,
  MathError,
  NO_DEBT_HEALTH_FACTOR,
  SECONDS_PER_YEAR,
  accrueIndex,
  applyBps,
  healthFactorBps,
  maxBorrowAmount,
  maxRepayable,
  ratioBps,
  seizeAmount,
  simpleInterest,
  utilizationBps,
} from '../dist/index.js';

test('bps helpers match the Rust scale', () => {
  assert.equal(applyBps(20_000n, 8_000n), 16_000n);
  assert.equal(ratioBps(1n, 4n), 2_500n);
  assert.equal(ratioBps(0n, 0n), 0n);
  assert.equal(BPS_DIVISOR, 10_000n);
});

test('utilisation is clamped and debt-free health is sentinel', () => {
  assert.equal(utilizationBps(5_000n, 20_000n), 2_500n);
  assert.equal(utilizationBps(30_000n, 20_000n), BPS_DIVISOR);
  assert.equal(healthFactorBps(20_000n, 0n, 8_000n), NO_DEBT_HEALTH_FACTOR);
  assert.equal(healthFactorBps(20_000n, 10_000n, 8_000n), 16_000n);
});

test('interest and liquidation sizing mirror contract math', () => {
  assert.equal(simpleInterest(1_000n, 1_000n, SECONDS_PER_YEAR), 100n);
  assert.equal(accrueIndex(INDEX_SCALE, 1_000n, SECONDS_PER_YEAR), INDEX_SCALE + INDEX_SCALE / 10n);
  assert.equal(seizeAmount(1_000n, 500n), 1_050n);
  assert.equal(maxRepayable(1_000n, 5_000n, 900n), 500n);
  assert.equal(maxBorrowAmount(20_000n, 7_500n, 1_000n), 14_000n);
});

test('invalid inputs raise MathError', () => {
  assert.throws(() => applyBps(-1n, 100n), MathError);
  assert.throws(() => utilizationBps(1n, -1n), MathError);
  assert.throws(() => healthFactorBps(1n, 1n, 0n), MathError);
});
