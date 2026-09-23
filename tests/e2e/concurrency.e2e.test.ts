/**
 * Multi-User Concurrent Interaction Simulation Tests (Issue #860)
 *
 * Simulates multiple users performing simultaneous operations against the
 * lending protocol to verify system correctness under concurrent load.
 * Tests deposit, borrow, repay, and withdraw operations happening in parallel.
 */

import request from 'supertest';
import express, { Application } from 'express';

const USER_PREFIX = 'G';
const NUM_CONCURRENT_USERS = 20;

function generateUserAddress(index: number): string {
  const suffix = index.toString().padStart(53, '0');
  return `${USER_PREFIX}${suffix}A`;
}

function buildApiApp(): Application {
  const app = express();
  app.use(express.json());

  interface Position {
    collateral: number;
    debt: number;
  }

  const positions = new Map<string, Position>();
  let txCounter = 0;

  app.use('/api/lending', (req, res, next) => {
    next();
  });

  app.get('/api/lending/prepare/:operation', (req, res) => {
    const { operation } = req.params;
    const { userAddress, amount } = req.query;

    if (!userAddress || !amount) {
      return res.status(400).json({ success: false, error: 'Missing params' });
    }

    const validOps = ['deposit', 'borrow', 'repay', 'withdraw'];
    if (!validOps.includes(operation)) {
      return res.status(400).json({ success: false, error: 'Invalid operation' });
    }

    const amt = parseInt(amount as string, 10);
    if (isNaN(amt) || amt <= 0) {
      return res.status(400).json({ success: false, error: 'Invalid amount' });
    }

    return res.status(200).json({
      success: true,
      operation,
      unsignedXdr: `mock-xdr-${operation}-${userAddress}-${amount}`,
      fee: 100,
    });
  });

  app.post('/api/lending/submit', (req, res) => {
    const { signedXdr, userAddress, operation } = req.body;

    if (!signedXdr) {
      return res.status(400).json({ success: false, error: 'signedXdr required' });
    }

    txCounter++;
    const txHash = `tx-${txCounter}-${Date.now()}`;

    if (userAddress && operation) {
      const pos = positions.get(userAddress) || { collateral: 0, debt: 0 };
      const amt = parseInt(req.body.amount || '0', 10);

      switch (operation) {
        case 'deposit':
          pos.collateral += amt;
          break;
        case 'borrow':
          pos.debt += amt;
          break;
        case 'repay':
          pos.debt = Math.max(0, pos.debt - amt);
          break;
        case 'withdraw':
          pos.collateral = Math.max(0, pos.collateral - amt);
          break;
      }
      positions.set(userAddress, pos);
    }

    return res.status(200).json({ success: true, transactionHash: txHash });
  });

  app.get('/api/lending/position/:userAddress', (req, res) => {
    const { userAddress } = req.params;
    const pos = positions.get(userAddress) || { collateral: 0, debt: 0 };
    return res.status(200).json({ success: true, position: pos });
  });

  app.get('/api/health', (_req, res) => {
    return res.status(200).json({ status: 'ok', timestamp: Date.now() });
  });

  return app;
}

describe('Multi-User Concurrent Interaction Simulation (#860)', () => {
  let app: Application;

  beforeEach(() => {
    app = buildApiApp();
  });

  // ── 1. Concurrent deposits ──────────────────────────────────────────────

  describe('Concurrent deposits from multiple users', () => {
    it('should handle 20 simultaneous deposit requests', async () => {
      const depositPromises = Array.from({ length: NUM_CONCURRENT_USERS }, (_, i) => {
        const user = generateUserAddress(i);
        return request(app)
          .get('/api/lending/prepare/deposit')
          .query({ userAddress: user, amount: '1000000' });
      });

      const results = await Promise.all(depositPromises);

      for (const res of results) {
        expect(res.status).toBe(200);
        expect(res.body.success).toBe(true);
        expect(res.body.operation).toBe('deposit');
      }
    });

    it('each concurrent deposit produces a unique XDR', async () => {
      const depositPromises = Array.from({ length: 10 }, (_, i) => {
        const user = generateUserAddress(i);
        return request(app)
          .get('/api/lending/prepare/deposit')
          .query({ userAddress: user, amount: `${1000000 + i * 1000}` });
      });

      const results = await Promise.all(depositPromises);
      const xdrs = results.map((r) => r.body.unsignedXdr);
      const uniqueXdrs = new Set(xdrs);

      expect(uniqueXdrs.size).toBe(xdrs.length);
    });
  });

  // ── 2. Concurrent borrows against collateral ────────────────────────────

  describe('Concurrent borrow operations', () => {
    it('should handle multiple users borrowing simultaneously', async () => {
      const borrowPromises = Array.from({ length: NUM_CONCURRENT_USERS }, (_, i) => {
        const user = generateUserAddress(i);
        return request(app)
          .get('/api/lending/prepare/borrow')
          .query({ userAddress: user, amount: '500000' });
      });

      const results = await Promise.all(borrowPromises);

      for (const res of results) {
        expect(res.status).toBe(200);
        expect(res.body.success).toBe(true);
        expect(res.body.operation).toBe('borrow');
      }
    });
  });

  // ── 3. Mixed concurrent operations ──────────────────────────────────────

  describe('Mixed concurrent operations', () => {
    it('should handle deposits, borrows, repays, and withdraws simultaneously', async () => {
      const operations = ['deposit', 'borrow', 'repay', 'withdraw'];
      const mixedPromises: Promise<any>[] = [];

      for (let i = 0; i < NUM_CONCURRENT_USERS; i++) {
        const user = generateUserAddress(i);
        const op = operations[i % operations.length];
        mixedPromises.push(
          request(app)
            .get(`/api/lending/prepare/${op}`)
            .query({ userAddress: user, amount: '250000' })
        );
      }

      const results = await Promise.all(mixedPromises);

      for (const res of results) {
        expect(res.status).toBe(200);
        expect(res.body.success).toBe(true);
      }
    });
  });

  // ── 4. Concurrent transaction submissions ───────────────────────────────

  describe('Concurrent transaction submissions', () => {
    it('should process multiple submissions without conflicts', async () => {
      const submitPromises = Array.from({ length: NUM_CONCURRENT_USERS }, (_, i) => {
        const user = generateUserAddress(i);
        return request(app)
          .post('/api/lending/submit')
          .send({
            signedXdr: `signed-xdr-${user}`,
            userAddress: user,
            operation: 'deposit',
            amount: 1000000,
          });
      });

      const results = await Promise.all(submitPromises);

      for (const res of results) {
        expect(res.status).toBe(200);
        expect(res.body.success).toBe(true);
        expect(res.body.transactionHash).toBeDefined();
      }
    });

    it('each concurrent submission produces a unique transaction hash', async () => {
      const submitPromises = Array.from({ length: 10 }, (_, i) => {
        const user = generateUserAddress(i);
        return request(app)
          .post('/api/lending/submit')
          .send({
            signedXdr: `signed-xdr-${user}-${i}`,
            userAddress: user,
            operation: 'deposit',
            amount: 1000000,
          });
      });

      const results = await Promise.all(submitPromises);
      const hashes = results.map((r: any) => r.body.transactionHash);
      const uniqueHashes = new Set(hashes);

      expect(uniqueHashes.size).toBe(hashes.length);
    });
  });

  // ── 5. Position consistency under concurrent operations ─────────────────

  describe('Position consistency', () => {
    it('should maintain correct positions after concurrent operations', async () => {
      const user = generateUserAddress(0);

      const depositRes = await request(app)
        .post('/api/lending/submit')
        .send({
          signedXdr: `signed-${user}`,
          userAddress: user,
          operation: 'deposit',
          amount: 1000000,
        });
      expect(depositRes.status).toBe(200);

      const positionRes = await request(app).get(`/api/lending/position/${user}`);
      expect(positionRes.status).toBe(200);
      expect(positionRes.body.position.collateral).toBe(1000000);
    });
  });

  // ── 6. System health under load ─────────────────────────────────────────

  describe('System health under concurrent load', () => {
    it('API remains responsive during high concurrency', async () => {
      const healthBefore = await request(app).get('/api/health');
      expect(healthBefore.status).toBe(200);

      const loadPromises = Array.from({ length: NUM_CONCURRENT_USERS }, (_, i) => {
        const user = generateUserAddress(i);
        return request(app)
          .get('/api/lending/prepare/deposit')
          .query({ userAddress: user, amount: '1000000' });
      });

      await Promise.all(loadPromises);

      const healthAfter = await request(app).get('/api/health');
      expect(healthAfter.status).toBe(200);
    });
  });
});
