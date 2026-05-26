import request from 'supertest';
import app, { resetRateLimiters } from '../app';

describe('API response compression', () => {
  beforeEach(async () => {
    await resetRateLimiters();
  });

  it('compresses large JSON responses when the client accepts gzip', async () => {
    const response = await request(app)
      .get('/api/openapi.json')
      .set('Accept-Encoding', 'gzip');

    expect(response.status).toBe(200);
    expect(response.headers['content-encoding']).toBe('gzip');
  });
});
