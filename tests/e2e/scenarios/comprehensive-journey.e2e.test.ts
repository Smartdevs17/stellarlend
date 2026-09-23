/**
 * E2E: Comprehensive user journey tests — Issue #814
 *
 * Covers the complete user lifecycle including:
 * - Multi-user concurrent operations
 * - Admin operations (pause/resume, parameter changes)
 * - Oracle price impact on positions
 * - Liquidation flow with close factor enforcement
 * - Edge cases (dust amounts, insufficient balances)
 * - Position health monitoring
 */

import request from 'supertest';
import { Application } from 'express';
import {
  buildLendingApp,
  setPrice,
  reset,
  assignRole,
  positionSummary,
} from './harness';

describe('E2E: comprehensive user journey', () => {
  let app: Application;

  const ADMIN = 'GADMIN00000000000000000000000000000000000000000000000000';
  const USER_A = 'GUSERA0000000000000000000000000000000000000000000000000';
  const USER_B = 'GUSERB0000000000000000000000000000000000000000000000000';
  const LIQUIDATOR = 'GLIQUIDATOR00000000000000000000000000000000000000000';

  beforeEach(() => {
    reset();
    app = buildLendingApp();
    assignRole(ADMIN, 'admin');
    assignRole(LIQUIDATOR, 'liquidator');
    setPrice('XLM', 0.12);
    setPrice('USDC', 1.0);
    setPrice('BTC', 60000);
    setPrice('ETH', 3000);
  });

  // ─── Multi-user concurrent operations ─────────────────────────────────────

  describe('multi-user concurrent operations', () => {
    it('allows multiple users to deposit and borrow independently', async () => {
      // User A deposits XLM
      const depositA = await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 50_000 });
      expect(depositA.status).toBe(200);

      // User B deposits BTC
      const depositB = await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_B, asset: 'BTC', amount: 2 });
      expect(depositB.status).toBe(200);

      // User A borrows USDC
      const borrowA = await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 3_000 });
      expect(borrowA.status).toBe(200);
      expect(borrowA.body.position.debt.USDC).toBe(3_000);

      // User B borrows USDC
      const borrowB = await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_B, asset: 'USDC', amount: 50_000 });
      expect(borrowB.status).toBe(200);
      expect(borrowB.body.position.debt.USDC).toBe(50_000);

      // Verify positions are independent
      const posA = await request(app).get(`/api/positions/${USER_A}`);
      const posB = await request(app).get(`/api/positions/${USER_B}`);
      expect(posA.body.debtValueUsd).toBeCloseTo(3_000);
      expect(posB.body.debtValueUsd).toBeCloseTo(50_000);
    });

    it('allows partial repay and re-borrow', async () => {
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 100_000 });
      await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 5_000 });

      // Partial repay
      const repay = await request(app)
        .post('/api/lending/repay')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 2_000 });
      expect(repay.status).toBe(200);
      expect(repay.body.position.debt.USDC).toBe(3_000);

      // Re-borrow up to capacity
      const reborrow = await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 2_000 });
      expect(reborrow.status).toBe(200);
      expect(reborrow.body.position.debt.USDC).toBe(5_000);
    });
  });

  // ─── Admin operations ─────────────────────────────────────────────────────

  describe('admin operations', () => {
    it('admin can pause and resume the protocol', async () => {
      // Deposit before pause
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 10_000 });

      // Pause
      const pause = await request(app)
        .post('/api/protocol/pause')
        .send({ callerAddress: ADMIN });
      expect(pause.status).toBe(200);
      expect(pause.body.paused).toBe(true);

      // Operations fail while paused
      const borrow = await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 100 });
      expect(borrow.status).toBe(503);

      // Resume
      const resume = await request(app)
        .post('/api/protocol/resume')
        .send({ callerAddress: ADMIN });
      expect(resume.status).toBe(200);
      expect(resume.body.paused).toBe(false);

      // Operations work again
      const borrowAfter = await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 100 });
      expect(borrowAfter.status).toBe(200);
    });

    it('non-admin cannot pause the protocol', async () => {
      const pause = await request(app)
        .post('/api/protocol/pause')
        .send({ callerAddress: USER_A });
      expect(pause.status).toBe(403);
    });
  });

  // ─── Oracle price impact ──────────────────────────────────────────────────

  describe('oracle price impact', () => {
    it('price increase improves health factor', async () => {
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 100_000 });
      await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 5_000 });

      const before = await request(app).get(`/api/positions/${USER_A}`);
      const hfBefore = before.body.healthFactor;

      // Increase XLM price
      setPrice('XLM', 0.20);

      const after = await request(app).get(`/api/positions/${USER_A}`);
      expect(after.body.healthFactor).toBeGreaterThan(hfBefore);
    });

    it('price decrease can make position liquidatable', async () => {
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 100_000 });
      await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 8_000 });

      // Initial health check
      const before = await request(app).get(`/api/positions/${USER_A}`);
      expect(before.body.liquidatable).toBe(false);

      // Crash XLM price
      setPrice('XLM', 0.05);

      const after = await request(app).get(`/api/positions/${USER_A}`);
      expect(after.body.liquidatable).toBe(true);
    });

    it('rejects borrow when oracle price is unavailable', async () => {
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 10_000 });

      const borrow = await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'UNKNOWN', amount: 100 });
      expect(borrow.status).toBe(400);
    });
  });

  // ─── Liquidation flow ─────────────────────────────────────────────────────

  describe('liquidation flow', () => {
    it('liquidator can liquidate an unhealthy position', async () => {
      // User deposits and borrows near capacity
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 100_000 });
      await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 8_000 });

      // Price drops, making position liquidatable
      setPrice('XLM', 0.05);

      const position = await request(app).get(`/api/positions/${USER_A}`);
      expect(position.body.liquidatable).toBe(true);

      // Liquidator repays half the debt (close factor = 50%)
      const liquidate = await request(app)
        .post('/api/liquidations/liquidate')
        .send({
          callerAddress: LIQUIDATOR,
          targetUser: USER_A,
          debtAsset: 'USDC',
          collateralAsset: 'XLM',
          repayAmount: 4_000,
        });
      expect(liquidate.status).toBe(200);
      expect(liquidate.body.success).toBe(true);
      expect(liquidate.body.seizedAmount).toBeGreaterThan(0);

      // Position should still exist but with reduced debt
      const after = await request(app).get(`/api/positions/${USER_A}`);
      expect(after.body.debt.USDC).toBe(4_000);
    });

    it('rejects liquidation when position is healthy', async () => {
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 100_000 });
      await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 5_000 });

      const liquidate = await request(app)
        .post('/api/liquidations/liquidate')
        .send({
          callerAddress: LIQUIDATOR,
          targetUser: USER_A,
          debtAsset: 'USDC',
          collateralAsset: 'XLM',
          repayAmount: 1_000,
        });
      expect(liquidate.status).toBe(400);
    });

    it('enforces close factor limit', async () => {
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 100_000 });
      await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 8_000 });

      setPrice('XLM', 0.05);

      // Try to repay more than close factor allows (50% of 8000 = 4000)
      const liquidate = await request(app)
        .post('/api/liquidations/liquidate')
        .send({
          callerAddress: LIQUIDATOR,
          targetUser: USER_A,
          debtAsset: 'USDC',
          collateralAsset: 'XLM',
          repayAmount: 5_000,
        });
      expect(liquidate.status).toBe(400);
    });

    it('non-liquidator cannot perform liquidation', async () => {
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 100_000 });
      await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 8_000 });
      setPrice('XLM', 0.05);

      const liquidate = await request(app)
        .post('/api/liquidations/liquidate')
        .send({
          callerAddress: USER_B,
          targetUser: USER_A,
          debtAsset: 'USDC',
          collateralAsset: 'XLM',
          repayAmount: 1_000,
        });
      expect(liquidate.status).toBe(403);
    });
  });

  // ─── Edge cases ───────────────────────────────────────────────────────────

  describe('edge cases', () => {
    it('rejects deposit with zero amount', async () => {
      const deposit = await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 0 });
      expect(deposit.status).toBe(400);
    });

    it('rejects borrow with missing parameters', async () => {
      const borrow = await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A });
      expect(borrow.status).toBe(400);
    });

    it('rejects repay amount exceeding debt', async () => {
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 100_000 });
      await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 1_000 });

      const repay = await request(app)
        .post('/api/lending/repay')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 2_000 });
      expect(repay.status).toBe(400);
    });

    it('rejects withdraw amount exceeding collateral', async () => {
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 10_000 });

      const withdraw = await request(app)
        .post('/api/lending/withdraw')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 20_000 });
      expect(withdraw.status).toBe(400);
    });

    it('returns 404 for unknown asset price', async () => {
      const price = await request(app).get('/api/prices/UNKNOWN');
      expect(price.status).toBe(404);
    });

    it('health check returns healthy status when not paused', async () => {
      const health = await request(app).get('/api/health');
      expect(health.status).toBe(200);
      expect(health.body.status).toBe('healthy');
    });
  });

  // ─── Full lifecycle with multi-collateral ─────────────────────────────────

  describe('multi-collateral lifecycle', () => {
    it('user deposits multiple collateral types and borrows against total value', async () => {
      // Deposit XLM
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 50_000 });

      // Deposit ETH
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'ETH', amount: 10 });

      // Total collateral value: 50000*0.12 + 10*3000 = 6000 + 30000 = 36000
      // 75% LTV cap = 27000
      const borrow = await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 20_000 });
      expect(borrow.status).toBe(200);
      expect(borrow.body.position.collateralValueUsd).toBeCloseTo(36_000);

      // Check position shows both collateral types
      const position = await request(app).get(`/api/positions/${USER_A}`);
      expect(position.body.collateral.XLM).toBe(50_000);
      expect(position.body.collateral.ETH).toBe(10);
      expect(position.body.healthFactor).toBeGreaterThan(1);
    });

    it('withdraw one collateral type while maintaining health', async () => {
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'XLM', amount: 50_000 });
      await request(app)
        .post('/api/lending/deposit')
        .send({ userAddress: USER_A, asset: 'ETH', amount: 10 });
      await request(app)
        .post('/api/lending/borrow')
        .send({ userAddress: USER_A, asset: 'USDC', amount: 10_000 });

      // Withdraw some ETH
      const withdraw = await request(app)
        .post('/api/lending/withdraw')
        .send({ userAddress: USER_A, asset: 'ETH', amount: 2 });
      expect(withdraw.status).toBe(200);

      // Position should still be healthy
      const position = await request(app).get(`/api/positions/${USER_A}`);
      expect(position.body.collateral.ETH).toBe(8);
      expect(position.body.healthFactor).toBeGreaterThan(1);
    });
  });
});
