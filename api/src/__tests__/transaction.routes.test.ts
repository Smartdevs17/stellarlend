import express from 'express';
import request from 'supertest';
import transactionRoutes from '../routes/transaction.routes';
import { errorHandler } from '../middleware/errorHandler';
import { generateToken } from '../middleware/auth';
import {
  transactionBuilderService,
  resetTransactionStore,
} from '../services/transactionBuilder.service';
import { ForbiddenError } from '../utils/errors';

const app = express();
app.use(express.json());
app.use('/api/transactions', transactionRoutes);
app.use('/api/v1/account/transactions', transactionRoutes);
app.use(errorHandler);

const userA = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
const userB = 'GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBWHF';
const tokenA = generateToken(userA);
const tokenB = generateToken(userB);

const sampleSteps = [
  {
    operation: 'deposit' as const,
    amount: '1000000',
    assetAddress: 'CAS3J7GYLGXMF6TDJBBYYSE3VUMAS45GQQIT3WZ43STZOEB64HOAX6TR',
  },
];

describe('Transaction Workflow Routes Authorization', () => {
  beforeEach(() => {
    resetTransactionStore();
  });

  describe('Authentication Enforcement (401 Unauthorized)', () => {
    it('rejects POST / without token', async () => {
      const res = await request(app)
        .post('/api/transactions')
        .send({ steps: sampleSteps });

      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe('UNAUTHORIZED');
    });

    it('rejects GET /user/:userAddress without token', async () => {
      const res = await request(app).get(`/api/transactions/user/${userA}`);

      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe('UNAUTHORIZED');
    });

    it('rejects GET /:txId without token', async () => {
      const res = await request(app).get('/api/transactions/tx-123');

      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe('UNAUTHORIZED');
    });

    it('rejects POST /:txId/steps/:stepId/prepare without token', async () => {
      const res = await request(app).post('/api/transactions/tx-123/steps/step-123/prepare');

      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe('UNAUTHORIZED');
    });

    it('rejects POST /steps/approve without token', async () => {
      const res = await request(app)
        .post('/api/transactions/steps/approve')
        .send({ txId: 'tx-123', stepId: 'step-123', signedXdr: 'mock' });

      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe('UNAUTHORIZED');
    });

    it('rejects POST /steps/reject without token', async () => {
      const res = await request(app)
        .post('/api/transactions/steps/reject')
        .send({ txId: 'tx-123', stepId: 'step-123', reason: 'cancelled' });

      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe('UNAUTHORIZED');
    });

    it('rejects requests with invalid token', async () => {
      const res = await request(app)
        .get(`/api/transactions/user/${userA}`)
        .set('Authorization', 'Bearer invalid-token');

      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe('UNAUTHORIZED');
    });

    it('rejects unauthenticated requests on v1 path', async () => {
      const res = await request(app).get(`/api/v1/account/transactions/user/${userA}`);

      expect(res.status).toBe(401);
      expect(res.body.error.code).toBe('UNAUTHORIZED');
    });
  });

  describe('Identity Binding & Ownership Enforcement (403 Forbidden)', () => {
    it('rejects creation when caller specifies a different user address', async () => {
      const res = await request(app)
        .post('/api/transactions')
        .set('Authorization', `Bearer ${tokenA}`)
        .send({
          userAddress: userB,
          steps: sampleSteps,
        });

      expect(res.status).toBe(403);
      expect(res.body.error.code).toBe('FORBIDDEN');
    });

    it('rejects cross-user transaction listing', async () => {
      const res = await request(app)
        .get(`/api/transactions/user/${userB}`)
        .set('Authorization', `Bearer ${tokenA}`);

      expect(res.status).toBe(403);
      expect(res.body.error.code).toBe('FORBIDDEN');
    });

    it('rejects cross-user access to GET /:txId', async () => {
      const tx = transactionBuilderService.create({ steps: sampleSteps }, userA);

      const res = await request(app)
        .get(`/api/transactions/${tx.txId}`)
        .set('Authorization', `Bearer ${tokenB}`);

      expect(res.status).toBe(403);
      expect(res.body.error.code).toBe('FORBIDDEN');
    });

    it('rejects cross-user prepareStep', async () => {
      const tx = transactionBuilderService.create({ steps: sampleSteps }, userA);
      const stepId = tx.steps[0].stepId;

      const res = await request(app)
        .post(`/api/transactions/${tx.txId}/steps/${stepId}/prepare`)
        .set('Authorization', `Bearer ${tokenB}`);

      expect(res.status).toBe(403);
      expect(res.body.error.code).toBe('FORBIDDEN');
    });

    it('rejects cross-user approveStep', async () => {
      const tx = transactionBuilderService.create({ steps: sampleSteps }, userA);
      const stepId = tx.steps[0].stepId;

      const res = await request(app)
        .post('/api/transactions/steps/approve')
        .set('Authorization', `Bearer ${tokenB}`)
        .send({ txId: tx.txId, stepId, signedXdr: 'test-xdr' });

      expect(res.status).toBe(403);
      expect(res.body.error.code).toBe('FORBIDDEN');
    });

    it('rejects cross-user rejectStep', async () => {
      const tx = transactionBuilderService.create({ steps: sampleSteps }, userA);
      const stepId = tx.steps[0].stepId;

      const res = await request(app)
        .post('/api/transactions/steps/reject')
        .set('Authorization', `Bearer ${tokenB}`)
        .send({ txId: tx.txId, stepId, reason: 'cancelled' });

      expect(res.status).toBe(403);
      expect(res.body.error.code).toBe('FORBIDDEN');
    });
  });

  describe('Legitimate Owner Workflow (200/201 Success)', () => {
    it('creates transaction bound to authenticated caller', async () => {
      const res = await request(app)
        .post('/api/transactions')
        .set('Authorization', `Bearer ${tokenA}`)
        .send({ steps: sampleSteps });

      expect(res.status).toBe(201);
      expect(res.body.success).toBe(true);
      expect(res.body.transaction.userAddress).toBe(userA);
      expect(res.body.transaction.steps).toHaveLength(1);
    });

    it('retrieves transaction for owner', async () => {
      const tx = transactionBuilderService.create({ steps: sampleSteps }, userA);

      const res = await request(app)
        .get(`/api/transactions/${tx.txId}`)
        .set('Authorization', `Bearer ${tokenA}`);

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.transaction.txId).toBe(tx.txId);
    });

    it('lists transactions for owner', async () => {
      transactionBuilderService.create({ steps: sampleSteps }, userA);

      const res = await request(app)
        .get(`/api/transactions/user/${userA}`)
        .set('Authorization', `Bearer ${tokenA}`);

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.transactions).toHaveLength(1);
    });

    it('allows owner to reject step and fails transaction', async () => {
      const tx = transactionBuilderService.create({ steps: sampleSteps }, userA);
      const stepId = tx.steps[0].stepId;

      const res = await request(app)
        .post('/api/transactions/steps/reject')
        .set('Authorization', `Bearer ${tokenA}`)
        .send({ txId: tx.txId, stepId, reason: 'user cancelled' });

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.transaction.status).toBe('failed');
      expect(res.body.transaction.steps[0].status).toBe('rejected');
    });

    it('supports v1 account routes mount path', async () => {
      const res = await request(app)
        .post('/api/v1/account/transactions')
        .set('Authorization', `Bearer ${tokenA}`)
        .send({ steps: sampleSteps });

      expect(res.status).toBe(201);
      expect(res.body.success).toBe(true);
      expect(res.body.transaction.userAddress).toBe(userA);
    });
  });

  describe('Defense-in-Depth Service Layer Authorization', () => {
    it('service throws ForbiddenError when creating for different caller', () => {
      expect(() => {
        transactionBuilderService.create(
          { userAddress: userB, steps: sampleSteps },
          userA
        );
      }).toThrow(ForbiddenError);
    });

    it('service throws ForbiddenError when listing for different caller', () => {
      expect(() => {
        transactionBuilderService.listForUser(userB, userA);
      }).toThrow(ForbiddenError);
    });

    it('service throws ForbiddenError when getting tx of another caller', () => {
      const tx = transactionBuilderService.create({ steps: sampleSteps }, userA);
      expect(() => {
        transactionBuilderService.getTransaction(tx.txId, userB);
      }).toThrow(ForbiddenError);
    });

    it('service throws ForbiddenError when rejecting step of another caller', () => {
      const tx = transactionBuilderService.create({ steps: sampleSteps }, userA);
      const stepId = tx.steps[0].stepId;
      expect(() => {
        transactionBuilderService.rejectStep(
          { txId: tx.txId, stepId, reason: 'abort' },
          userB
        );
      }).toThrow(ForbiddenError);
    });

    it('service throws ForbiddenError when preparing step of another caller', async () => {
      const tx = transactionBuilderService.create({ steps: sampleSteps }, userA);
      const stepId = tx.steps[0].stepId;
      await expect(
        transactionBuilderService.prepareStep(tx.txId, stepId, userB)
      ).rejects.toThrow(ForbiddenError);
    });

    it('service throws ForbiddenError when approving step of another caller', async () => {
      const tx = transactionBuilderService.create({ steps: sampleSteps }, userA);
      const stepId = tx.steps[0].stepId;
      await expect(
        transactionBuilderService.approveStep(
          { txId: tx.txId, stepId, signedXdr: 'signed' },
          userB
        )
      ).rejects.toThrow(ForbiddenError);
    });
  });
});
