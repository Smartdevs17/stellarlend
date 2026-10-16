/**
 * Position lifecycle endpoint tests for cross-chain lending positions.
 *
 * Covers the full lifecycle exposed under /api/bridge/lending:
 *   open   → POST /positions/open
 *   adjust → POST /collateral/lock, POST /collateral/release
 *   repay  → POST /positions/repaid
 *   close  → POST /positions/cancel
 *   read   → GET  /positions/:userAddress/:remoteChain/:remotePool
 *
 * Every path asserts both the success response and its error responses
 * (missing fields, protocol pause, and unexpected failures).
 */
import request from 'supertest';
import app from '../app';
import { emergencyPauseService } from '../services/emergencyPause.service';
import { auditLogService } from '../services/auditLog.service';

const BASE = '/api/bridge/lending';

const USER = 'GUSERADDRESSFORPOSITIONLIFECYCLETESTS000000000000000000000';
const RELAYER = 'GRELAYERADDRESSFORPOSITIONLIFECYCLETESTS0000000000000000';
const ADMIN = 'GADMINADDRESSFORPOSITIONLIFECYCLETESTS000000000000000000';

const positionKey = {
  userAddress: USER,
  remoteChain: 'ethereum',
  remotePool: 'aave-v3',
};

const openBody = {
  ...positionKey,
  bridgeId: 'wormhole',
  collateralAsset: 'XLM',
  collateralAmount: '1000000000',
  borrowAsset: 'USDC',
  borrowAmount: '250000000',
};

const lockBody = {
  callerAddress: RELAYER,
  ...positionKey,
  asset: 'XLM',
  vaultContract: 'CVAULTCONTRACT',
  amount: '500000000',
};

const releaseBody = { callerAddress: RELAYER, ...positionKey };
const repaidBody = { callerAddress: RELAYER, ...positionKey };
const cancelBody = { adminAddress: ADMIN, ...positionKey };

function without<T extends Record<string, unknown>>(body: T, field: keyof T): Partial<T> {
  const copy: Partial<T> = { ...body };
  delete copy[field];
  return copy;
}

describe('Bridge lending position lifecycle endpoints', () => {
  let recordSpy: jest.SpyInstance;

  beforeEach(() => {
    if (emergencyPauseService.isPaused().paused) emergencyPauseService.resume();
    recordSpy = jest.spyOn(auditLogService, 'record');
  });

  afterEach(() => {
    jest.restoreAllMocks();
    if (emergencyPauseService.isPaused().paused) emergencyPauseService.resume();
  });

  // ─── Open ──────────────────────────────────────────────────────────────────

  describe('POST /positions/open', () => {
    it('opens an active position with defaults for optional fields', async () => {
      const res = await request(app).post(`${BASE}/positions/open`).send(openBody);

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.message).toBe('Cross-chain lending position opened');
      expect(res.body.position).toMatchObject({
        ...openBody,
        interestRateBps: 0,
        bridgeMessageId: null,
        status: 'active',
      });
      expect(new Date(res.body.position.createdAt).toISOString()).toBe(res.body.position.createdAt);
    });

    it('preserves provided interestRateBps and bridgeMessageId', async () => {
      const res = await request(app)
        .post(`${BASE}/positions/open`)
        .send({ ...openBody, interestRateBps: 450, bridgeMessageId: 42 });

      expect(res.status).toBe(200);
      expect(res.body.position.interestRateBps).toBe(450);
      expect(res.body.position.bridgeMessageId).toBe(42);
    });

    it('records an audit entry for the user', async () => {
      await request(app).post(`${BASE}/positions/open`).send(openBody);

      expect(recordSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          action: 'BRIDGE_LENDING_POSITION_OPEN',
          actor: USER,
          status: 'success',
        })
      );
    });

    it.each(Object.keys(openBody) as (keyof typeof openBody)[])(
      'returns 400 when %s is missing',
      async (field) => {
        const res = await request(app)
          .post(`${BASE}/positions/open`)
          .send(without(openBody, field));

        expect(res.status).toBe(400);
        expect(res.body.success).toBe(false);
        expect(res.body.error).toMatch(/are required/);
        expect(res.body.error).toContain(field);
        expect(recordSpy).not.toHaveBeenCalled();
      }
    );

    it('returns 400 for an empty body', async () => {
      const res = await request(app).post(`${BASE}/positions/open`).send({});

      expect(res.status).toBe(400);
      expect(res.body.success).toBe(false);
    });

    it('returns 503 while the protocol is paused', async () => {
      emergencyPauseService.pause('manual');

      const res = await request(app).post(`${BASE}/positions/open`).send(openBody);

      expect(res.status).toBe(503);
      expect(res.body).toEqual({ success: false, error: 'Protocol is paused', reason: 'manual' });
      expect(recordSpy).not.toHaveBeenCalled();
    });

    it('checks the pause state before validating the body', async () => {
      emergencyPauseService.pause('oracle-failure');

      const res = await request(app).post(`${BASE}/positions/open`).send({});

      expect(res.status).toBe(503);
      expect(res.body.reason).toBe('oracle-failure');
    });

    it('forwards unexpected errors to the error handler', async () => {
      recordSpy.mockImplementation(() => {
        throw new Error('audit store unavailable');
      });

      const res = await request(app).post(`${BASE}/positions/open`).send(openBody);

      expect(res.status).toBe(500);
      expect(res.body.success).toBe(false);
      expect(res.body.error).toEqual({
        code: 'INTERNAL_SERVER_ERROR',
        message: 'Internal server error',
      });
      expect(JSON.stringify(res.body)).not.toContain('audit store unavailable');
    });
  });

  // ─── Read ──────────────────────────────────────────────────────────────────

  describe('GET /positions/:userAddress/:remoteChain/:remotePool', () => {
    it('echoes the position key', async () => {
      const res = await request(app).get(
        `${BASE}/positions/${USER}/${positionKey.remoteChain}/${positionKey.remotePool}`
      );

      expect(res.status).toBe(200);
      expect(res.body).toEqual({ success: true, position: null, ...positionKey });
    });

    it('returns 404 when the key is incomplete', async () => {
      const res = await request(app).get(`${BASE}/positions/${USER}/${positionKey.remoteChain}`);

      expect(res.status).toBe(404);
    });
  });

  // ─── Adjust ────────────────────────────────────────────────────────────────

  describe('POST /collateral/lock (adjust: add collateral)', () => {
    it('records an unreleased collateral lock', async () => {
      const res = await request(app).post(`${BASE}/collateral/lock`).send(lockBody);

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.message).toBe('Collateral lock recorded');
      expect(res.body.lock).toMatchObject({
        userAddress: USER,
        asset: lockBody.asset,
        vaultContract: lockBody.vaultContract,
        amount: lockBody.amount,
        remoteChain: positionKey.remoteChain,
        remotePool: positionKey.remotePool,
        released: false,
      });
      expect(typeof res.body.lock.lockedAt).toBe('string');
      expect(res.body.lock).not.toHaveProperty('callerAddress');
      expect(recordSpy).toHaveBeenCalledWith(
        expect.objectContaining({ action: 'BRIDGE_COLLATERAL_LOCK', actor: USER })
      );
    });

    it.each(Object.keys(lockBody) as (keyof typeof lockBody)[])(
      'returns 400 when %s is missing',
      async (field) => {
        const res = await request(app)
          .post(`${BASE}/collateral/lock`)
          .send(without(lockBody, field));

        expect(res.status).toBe(400);
        expect(res.body.success).toBe(false);
        expect(res.body.error).toContain(field);
      }
    );

    it('returns 503 while the protocol is paused', async () => {
      emergencyPauseService.pause('manual');

      const res = await request(app).post(`${BASE}/collateral/lock`).send(lockBody);

      expect(res.status).toBe(503);
      expect(res.body.error).toBe('Protocol is paused');
    });
  });

  describe('POST /collateral/release (adjust: remove collateral)', () => {
    it('releases the collateral lock', async () => {
      const res = await request(app).post(`${BASE}/collateral/release`).send(releaseBody);

      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({
        success: true,
        ...positionKey,
        released: true,
        message: 'Collateral released',
      });
      expect(typeof res.body.releasedAt).toBe('string');
      expect(recordSpy).toHaveBeenCalledWith(
        expect.objectContaining({ action: 'BRIDGE_COLLATERAL_RELEASE', actor: USER })
      );
    });

    it.each(Object.keys(releaseBody) as (keyof typeof releaseBody)[])(
      'returns 400 when %s is missing',
      async (field) => {
        const res = await request(app)
          .post(`${BASE}/collateral/release`)
          .send(without(releaseBody, field));

        expect(res.status).toBe(400);
        expect(res.body.success).toBe(false);
        expect(res.body.error).toContain(field);
      }
    );

    it('returns 503 while the protocol is paused', async () => {
      emergencyPauseService.pause('manual');

      const res = await request(app).post(`${BASE}/collateral/release`).send(releaseBody);

      expect(res.status).toBe(503);
      expect(res.body.error).toBe('Protocol is paused');
    });
  });

  // ─── Repay ─────────────────────────────────────────────────────────────────

  describe('POST /positions/repaid', () => {
    it('marks the position repaid and audits the caller', async () => {
      const res = await request(app).post(`${BASE}/positions/repaid`).send(repaidBody);

      expect(res.status).toBe(200);
      expect(res.body).toEqual({
        success: true,
        ...positionKey,
        status: 'repaid',
        message: 'Position marked as repaid',
      });
      expect(recordSpy).toHaveBeenCalledWith(
        expect.objectContaining({ action: 'BRIDGE_LENDING_POSITION_REPAID', actor: RELAYER })
      );
    });

    it.each(Object.keys(repaidBody) as (keyof typeof repaidBody)[])(
      'returns 400 when %s is missing',
      async (field) => {
        const res = await request(app)
          .post(`${BASE}/positions/repaid`)
          .send(without(repaidBody, field));

        expect(res.status).toBe(400);
        expect(res.body.success).toBe(false);
        expect(res.body.error).toContain(field);
        expect(recordSpy).not.toHaveBeenCalled();
      }
    );

    it('still accepts repayment while the protocol is paused', async () => {
      emergencyPauseService.pause('manual');

      const res = await request(app).post(`${BASE}/positions/repaid`).send(repaidBody);

      expect(res.status).toBe(200);
      expect(res.body.status).toBe('repaid');
    });

    it('forwards unexpected errors to the error handler', async () => {
      recordSpy.mockImplementation(() => {
        throw new Error('audit store unavailable');
      });

      const res = await request(app).post(`${BASE}/positions/repaid`).send(repaidBody);

      expect(res.status).toBe(500);
      expect(res.body.success).toBe(false);
      expect(res.body.error).toEqual({
        code: 'INTERNAL_SERVER_ERROR',
        message: 'Internal server error',
      });
      expect(JSON.stringify(res.body)).not.toContain('audit store unavailable');
    });
  });

  // ─── Close ─────────────────────────────────────────────────────────────────

  describe('POST /positions/cancel (close)', () => {
    it('cancels the position and audits the admin', async () => {
      const res = await request(app).post(`${BASE}/positions/cancel`).send(cancelBody);

      expect(res.status).toBe(200);
      expect(res.body).toEqual({
        success: true,
        ...positionKey,
        status: 'cancelled',
        message: 'Position cancelled',
      });
      expect(recordSpy).toHaveBeenCalledWith(
        expect.objectContaining({ action: 'BRIDGE_LENDING_POSITION_CANCEL', actor: ADMIN })
      );
    });

    it.each(Object.keys(cancelBody) as (keyof typeof cancelBody)[])(
      'returns 400 when %s is missing',
      async (field) => {
        const res = await request(app)
          .post(`${BASE}/positions/cancel`)
          .send(without(cancelBody, field));

        expect(res.status).toBe(400);
        expect(res.body.success).toBe(false);
        expect(res.body.error).toContain(field);
        expect(recordSpy).not.toHaveBeenCalled();
      }
    );

    it('still allows an admin to close a position while paused', async () => {
      emergencyPauseService.pause('manual');

      const res = await request(app).post(`${BASE}/positions/cancel`).send(cancelBody);

      expect(res.status).toBe(200);
      expect(res.body.status).toBe('cancelled');
    });

    it('forwards unexpected errors to the error handler', async () => {
      recordSpy.mockImplementation(() => {
        throw new Error('audit store unavailable');
      });

      const res = await request(app).post(`${BASE}/positions/cancel`).send(cancelBody);

      expect(res.status).toBe(500);
      expect(res.body.success).toBe(false);
      expect(res.body.error).toEqual({
        code: 'INTERNAL_SERVER_ERROR',
        message: 'Internal server error',
      });
      expect(JSON.stringify(res.body)).not.toContain('audit store unavailable');
    });
  });

  // ─── End-to-end lifecycle ──────────────────────────────────────────────────

  describe('full lifecycle', () => {
    it('runs open → adjust → repay → release → close in order', async () => {
      const opened = await request(app).post(`${BASE}/positions/open`).send(openBody);
      expect(opened.status).toBe(200);
      expect(opened.body.position.status).toBe('active');

      const locked = await request(app).post(`${BASE}/collateral/lock`).send(lockBody);
      expect(locked.status).toBe(200);
      expect(locked.body.lock.released).toBe(false);

      const repaid = await request(app).post(`${BASE}/positions/repaid`).send(repaidBody);
      expect(repaid.status).toBe(200);
      expect(repaid.body.status).toBe('repaid');

      const released = await request(app).post(`${BASE}/collateral/release`).send(releaseBody);
      expect(released.status).toBe(200);
      expect(released.body.released).toBe(true);

      const closed = await request(app).post(`${BASE}/positions/cancel`).send(cancelBody);
      expect(closed.status).toBe(200);
      expect(closed.body.status).toBe('cancelled');

      expect(recordSpy.mock.calls.map(([entry]) => entry.action)).toEqual([
        'BRIDGE_LENDING_POSITION_OPEN',
        'BRIDGE_COLLATERAL_LOCK',
        'BRIDGE_LENDING_POSITION_REPAID',
        'BRIDGE_COLLATERAL_RELEASE',
        'BRIDGE_LENDING_POSITION_CANCEL',
      ]);
    });

    it('blocks opening and adjusting mid-lifecycle once paused, but allows repay and close', async () => {
      await request(app).post(`${BASE}/positions/open`).send(openBody).expect(200);

      emergencyPauseService.pause('governance-vote');

      await request(app).post(`${BASE}/positions/open`).send(openBody).expect(503);
      await request(app).post(`${BASE}/collateral/lock`).send(lockBody).expect(503);
      await request(app).post(`${BASE}/collateral/release`).send(releaseBody).expect(503);
      await request(app).post(`${BASE}/positions/repaid`).send(repaidBody).expect(200);
      await request(app).post(`${BASE}/positions/cancel`).send(cancelBody).expect(200);
    });
  });
});
