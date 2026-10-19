import {
  webhookService,
  signPayload,
  verifySignature,
  retryDelayMs,
} from '../services/webhook.service';

jest.mock('../utils/logger');

beforeEach(() => {
  webhookService._reset();
});

describe('signed webhook delivery (#1376)', () => {
  it('signs with HMAC-SHA256 over timestamp.body and verifies in constant time', () => {
    const secret = 'super-secret';
    const body = JSON.stringify({ hello: 'world' });
    const timestamp = Math.floor(Date.now() / 1000);
    const header = signPayload(secret, timestamp, body);

    expect(header).toMatch(/^t=\d+,v1=[0-9a-f]{64}$/);
    expect(verifySignature(secret, body, header)).toBe(true);
    expect(verifySignature('wrong-secret', body, header)).toBe(false);
    expect(verifySignature(secret, body + 'x', header)).toBe(false);
  });

  it('rejects signatures outside the replay tolerance window', () => {
    const secret = 's';
    const body = '{}';
    const old = Math.floor(Date.now() / 1000) - 600;
    expect(verifySignature(secret, body, signPayload(secret, old, body))).toBe(false);
  });

  it('delivers to matching subscriptions with signature headers', async () => {
    const { subscription } = webhookService.subscribe({
      url: 'https://example.test/hook',
      events: ['liquidation.alert'],
    });

    const fetcher = jest.fn().mockResolvedValue({ ok: true, status: 200 });
    const deliveries = await webhookService.dispatch(
      'liquidation.alert',
      { alerts: [{ address: 'G...', healthFactor: 0.9 }] },
      { fetcher }
    );

    expect(deliveries).toHaveLength(1);
    expect(deliveries[0]).toMatchObject({ status: 'delivered', attempts: 1, statusCode: 200 });
    expect(fetcher).toHaveBeenCalledTimes(1);

    const [url, init] = fetcher.mock.calls[0];
    expect(url).toBe('https://example.test/hook');
    expect(init.headers['X-StellarLend-Signature']).toMatch(/^t=\d+,v1=[0-9a-f]{64}$/);
    expect(init.headers['X-StellarLend-Event']).toBe('liquidation.alert');
    expect(subscription.id).toBeDefined();
  });

  it('does not deliver to subscriptions that did not request the event', async () => {
    webhookService.subscribe({ url: 'https://example.test/other', events: ['webhook.test'] });
    const fetcher = jest.fn().mockResolvedValue({ ok: true, status: 200 });

    const deliveries = await webhookService.dispatch('liquidation.alert', {}, { fetcher });
    expect(deliveries).toHaveLength(0);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it('retries failed deliveries up to 3 attempts with exponential backoff', async () => {
    webhookService.subscribe({ url: 'https://example.test/fail', events: ['*'] });
    const fetcher = jest
      .fn()
      .mockResolvedValueOnce({ ok: false, status: 500 })
      .mockResolvedValueOnce({ ok: false, status: 502 })
      .mockResolvedValue({ ok: false, status: 503 });

    const deliveries = await webhookService.dispatch('liquidation.alert', {}, { fetcher });

    expect(deliveries[0]).toMatchObject({ status: 'failed', attempts: 3, statusCode: 503 });
    expect(fetcher).toHaveBeenCalledTimes(3);
  }, 15000);

  it('caps the backoff delay and includes jitter', () => {
    expect(retryDelayMs(1)).toBeGreaterThanOrEqual(100);
    expect(retryDelayMs(10)).toBeLessThanOrEqual(2000 + 100);
  });

  it('keeps secrets out of listings and removes subscriptions', () => {
    const { subscription, secret } = webhookService.subscribe({ url: 'https://example.test/x' });
    expect(JSON.stringify(webhookService.list())).not.toContain(secret);
    expect(webhookService.unsubscribe(subscription.id)).toBe(true);
    expect(webhookService.list()).toHaveLength(0);
  });

  it('rejects invalid URLs', () => {
    expect(() => webhookService.subscribe({ url: 'ftp://nope' })).toThrow(/http/);
  });
});
