import request from 'supertest';
import app from '../app';

const VALID_ADDRESS = 'GDZZJ3UPZZCKY5DBH6ZGMPMRORRBG4ECIORASBUAXPPNCL4SYRHNLYU2';

describe('Validation Middleware', () => {
  describe('Prepare Validation (GET /api/lending/prepare/:operation)', () => {
    it('should reject empty userAddress', async () => {
      const response = await request(app)
        .get('/api/lending/prepare/deposit')
        .query({ amount: '1000000' });

      expect(response.status).toBe(400);
      expect(response.body.error).toBeDefined();
      expect(response.body.error).toContain('User address is required');
    });

    it('should reject invalid Stellar public key', async () => {
      const response = await request(app).get('/api/lending/prepare/deposit').query({
        userAddress: 'invalid-address',
        assetAddress: 'G...',
        amount: '100',
      });

      expect(response.status).toBe(400);
      expect(response.body.error).toBeDefined();
      expect(response.body.error).toContain('Invalid Stellar address');
    });

    it('should reject Stellar address with wrong prefix', async () => {
      const response = await request(app).get('/api/lending/prepare/deposit').query({
        userAddress: 'S...', // Secret key prefix
        assetAddress: 'G...',
        amount: '100',
      });

      expect(response.status).toBe(400);
      expect(response.body.error).toBeDefined();
      expect(response.body.error).toContain('Invalid Stellar address');
    });

    it('should accept valid Stellar public key', async () => {
      const response = await request(app).get('/api/lending/prepare/deposit').query({
        userAddress: VALID_ADDRESS,
        assetAddress: 'G...',
        amount: '100',
      });

      expect(response.status).not.toBe(400);
    });

    it('should reject missing amount', async () => {
      const response = await request(app).get('/api/lending/prepare/deposit').query({
        userAddress: VALID_ADDRESS,
        assetAddress: 'G...',
      });

      expect(response.status).toBe(400);
      expect(response.body.error).toContain('Amount is required');
    });

    it('should reject zero amount', async () => {
      const response = await request(app).get('/api/lending/prepare/deposit').query({
        userAddress: VALID_ADDRESS,
        assetAddress: 'G...',
        amount: '0',
      });

      expect(response.status).toBe(400);
      expect(response.body.error).toBeDefined();
      expect(response.body.error).toContain('Amount must be greater than 0');
    });

    it('should reject negative amount', async () => {
      const response = await request(app).get('/api/lending/prepare/deposit').query({
        userAddress: VALID_ADDRESS,
        assetAddress: 'G...',
        amount: '-1',
      });

      expect(response.status).toBe(400);
      expect(response.body.error).toBeDefined();
      expect(response.body.error).toContain('Amount must be greater than 0');
    });

    it('should reject invalid operation', async () => {
      const response = await request(app).get('/api/lending/prepare/invalid_op').query({
        userAddress: 'GDH7NBM22WUCYOLJJZ7ALN3QZ6W2G5YCHDP2YQWJZ76L2GZFPHSYZ4Y3',
        amount: '1000000',
      });

      expect(response.status).toBe(400);
    });
  });

  describe('Submit Validation (POST /api/lending/submit)', () => {
    it('should reject missing signedXdr', async () => {
      const response = await request(app).post('/api/lending/submit').send({});

      expect(response.status).toBe(400);
    });

    it('should reject empty signedXdr', async () => {
      const response = await request(app).post('/api/lending/submit').send({ signedXdr: '' });

      expect(response.status).toBe(400);
    });
  });
});
