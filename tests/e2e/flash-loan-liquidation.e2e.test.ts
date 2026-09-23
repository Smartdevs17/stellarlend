/**
 * Flash Loan + Liquidation Combo Integration Tests (Issue #861)
 *
 * Tests the integration between flash loan functionality and liquidation
 * mechanisms, verifying that flash-loan-funded liquidations work correctly,
 * profit calculations are accurate, and edge cases are handled.
 */

import request from 'supertest';
import express, { Application } from 'express';

const FLASH_LOAN_FEE_BPS = 9;
const LIQUIDATION_INCENTIVE_BPS = 500;

function buildFlashLoanApp(): Application {
  const app = express();
  app.use(express.json());

  interface LiquidationRecord {
    liquidator: string;
    borrower: string;
    debtAmount: number;
    collateralSeized: number;
    flashLoanFee: number;
    netProfit: number;
  }

  const liquidations: LiquidationRecord[] = [];

  app.post('/api/flash-loan/simulate', (req, res) => {
    const { debtAmount } = req.body;

    if (!debtAmount || debtAmount <= 0) {
      return res.status(400).json({ success: false, error: 'Invalid debt amount' });
    }

    const flashLoanFee = Math.round(debtAmount * FLASH_LOAN_FEE_BPS / 10000);
    const liquidationIncentive = Math.round(debtAmount * LIQUIDATION_INCENTIVE_BPS / 10000);
    const netProfit = liquidationIncentive - flashLoanFee;

    return res.status(200).json({
      success: true,
      data: {
        estimatedProfit: liquidationIncentive,
        flashLoanFee,
        liquidationIncentive,
        netProfit,
        isProfitable: netProfit > 0,
      },
    });
  });

  app.post('/api/flash-loan/execute', (req, res) => {
    const { liquidator, borrower, debtAmount } = req.body;

    if (!liquidator || !borrower || !debtAmount || debtAmount <= 0) {
      return res.status(400).json({ success: false, error: 'Missing required fields' });
    }

    const flashLoanFee = Math.round(debtAmount * FLASH_LOAN_FEE_BPS / 10000);
    const liquidationIncentive = Math.round(debtAmount * LIQUIDATION_INCENTIVE_BPS / 10000);
    const netProfit = liquidationIncentive - flashLoanFee;

    if (netProfit <= 0) {
      return res.status(400).json({
        success: false,
        error: 'Liquidation not profitable after flash loan fees',
      });
    }

    const collateralSeized = debtAmount + liquidationIncentive;

    const record: LiquidationRecord = {
      liquidator,
      borrower,
      debtAmount,
      collateralSeized,
      flashLoanFee,
      netProfit,
    };
    liquidations.push(record);

    return res.status(200).json({
      success: true,
      data: {
        liquidator,
        borrower,
        debtAmount,
        collateralSeized,
        flashLoanFee,
        netProfit,
        timestamp: Date.now(),
      },
    });
  });

  app.get('/api/flash-loan/liquidations', (_req, res) => {
    return res.status(200).json({
      success: true,
      data: liquidations,
      count: liquidations.length,
    });
  });

  app.get('/api/flash-loan/config', (_req, res) => {
    return res.status(200).json({
      success: true,
      data: {
        feeBps: FLASH_LOAN_FEE_BPS,
        maxAmount: 1_000_000_000_000,
        minAmount: 100,
      },
    });
  });

  return app;
}

describe('Flash Loan + Liquidation Combo Integration Tests (#861)', () => {
  let app: Application;

  beforeEach(() => {
    app = buildFlashLoanApp();
  });

  // ── 1. Simulation tests ─────────────────────────────────────────────────

  describe('Flash loan liquidation simulation', () => {
    it('should simulate profitable liquidation', async () => {
      const res = await request(app)
        .post('/api/flash-loan/simulate')
        .send({ debtAmount: 1_000_000 });

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.data.isProfitable).toBe(true);
      expect(res.body.data.netProfit).toBeGreaterThan(0);
    });

    it('should calculate correct flash loan fee', async () => {
      const res = await request(app)
        .post('/api/flash-loan/simulate')
        .send({ debtAmount: 100_000 });

      expect(res.status).toBe(200);
      expect(res.body.data.flashLoanFee).toBe(90);
    });

    it('should calculate correct liquidation incentive', async () => {
      const res = await request(app)
        .post('/api/flash-loan/simulate')
        .send({ debtAmount: 1_000_000 });

      expect(res.status).toBe(200);
      expect(res.body.data.liquidationIncentive).toBe(50_000);
    });

    it('should reject zero debt amount', async () => {
      const res = await request(app)
        .post('/api/flash-loan/simulate')
        .send({ debtAmount: 0 });

      expect(res.status).toBe(400);
    });

    it('should reject negative debt amount', async () => {
      const res = await request(app)
        .post('/api/flash-loan/simulate')
        .send({ debtAmount: -5000 });

      expect(res.status).toBe(400);
    });
  });

  // ── 2. Execution tests ──────────────────────────────────────────────────

  describe('Flash loan liquidation execution', () => {
    it('should execute profitable liquidation', async () => {
      const res = await request(app)
        .post('/api/flash-loan/execute')
        .send({
          liquidator: 'GLIQUIDATOR123',
          borrower: 'GBORROWER456',
          debtAmount: 1_000_000,
        });

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.data.netProfit).toBeGreaterThan(0);
      expect(res.body.data.collateralSeized).toBeGreaterThan(res.body.data.debtAmount);
    });

    it('should reject unprofitable liquidation', async () => {
      const res = await request(app)
        .post('/api/flash-loan/execute')
        .send({
          liquidator: 'GLIQUIDATOR123',
          borrower: 'GBORROWER456',
          debtAmount: 1,
        });

      expect(res.status).toBe(400);
    });

    it('should reject missing liquidator', async () => {
      const res = await request(app)
        .post('/api/flash-loan/execute')
        .send({ borrower: 'GBORROWER456', debtAmount: 1_000_000 });

      expect(res.status).toBe(400);
    });

    it('should reject missing borrower', async () => {
      const res = await request(app)
        .post('/api/flash-loan/execute')
        .send({ liquidator: 'GLIQUIDATOR123', debtAmount: 1_000_000 });

      expect(res.status).toBe(400);
    });
  });

  // ── 3. Liquidation history ──────────────────────────────────────────────

  describe('Liquidation history tracking', () => {
    it('should track executed liquidations', async () => {
      await request(app)
        .post('/api/flash-loan/execute')
        .send({
          liquidator: 'GLIQ1',
          borrower: 'GBOR1',
          debtAmount: 1_000_000,
        });

      await request(app)
        .post('/api/flash-loan/execute')
        .send({
          liquidator: 'GLIQ2',
          borrower: 'GBOR2',
          debtAmount: 2_000_000,
        });

      const res = await request(app).get('/api/flash-loan/liquidations');
      expect(res.status).toBe(200);
      expect(res.body.count).toBe(2);
    });
  });

  // ── 4. Configuration ────────────────────────────────────────────────────

  describe('Flash loan configuration', () => {
    it('should return correct fee configuration', async () => {
      const res = await request(app).get('/api/flash-loan/config');
      expect(res.status).toBe(200);
      expect(res.body.data.feeBps).toBe(FLASH_LOAN_FEE_BPS);
    });
  });

  // ── 5. Profit calculation accuracy ──────────────────────────────────────

  describe('Profit calculation accuracy', () => {
    it('net profit = liquidation incentive - flash loan fee', async () => {
      const debtAmount = 5_000_000;
      const res = await request(app)
        .post('/api/flash-loan/simulate')
        .send({ debtAmount });

      expect(res.status).toBe(200);
      const { liquidationIncentive, flashLoanFee, netProfit } = res.body.data;
      expect(netProfit).toBe(liquidationIncentive - flashLoanFee);
    });

    it('larger liquidations remain profitable', async () => {
      const res = await request(app)
        .post('/api/flash-loan/simulate')
        .send({ debtAmount: 100_000_000 });

      expect(res.status).toBe(200);
      expect(res.body.data.isProfitable).toBe(true);
    });
  });
});
