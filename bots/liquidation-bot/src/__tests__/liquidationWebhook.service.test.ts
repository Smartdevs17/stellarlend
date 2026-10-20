import axios from 'axios';
import { createHmac } from 'crypto';

import { Logger } from '../logger';
import {
  LiquidationWebhookService,
  signWebhookPayload,
} from '../liquidationWebhook.service';

jest.mock('axios');

const mockedAxios = axios as jest.Mocked<typeof axios>;

const liquidation = {
  positionAddress: 'GPOSITION',
  txHash: 'tx-123',
  profitStroops: 40000,
  timestamp: 1710000000000,
};

describe('LiquidationWebhookService', () => {
  const logger = new Logger('error');

  beforeEach(() => {
    jest.restoreAllMocks();
    mockedAxios.post.mockReset();
    mockedAxios.isAxiosError.mockImplementation((error: unknown) => Boolean((error as { response?: unknown })?.response));
  });

  it('signs the timestamp and exact raw body with HMAC-SHA256', () => {
    const timestamp = 1710000000;
    const body = '{"type":"liquidation.confirmed"}';
    const expected = createHmac('sha256', 'secret').update(`${timestamp}.${body}`).digest('hex');

    expect(signWebhookPayload('secret', timestamp, body)).toBe(`sha256=${expected}`);
  });

  it('delivers a signed confirmed-liquidation event', async () => {
    mockedAxios.post.mockResolvedValue({ status: 202 });
    const service = new LiquidationWebhookService(
      { url: 'https://example.test/hooks', secret: 'secret', maxAttempts: 3, retryDelayMs: 1 },
      logger,
      mockedAxios,
      jest.fn().mockResolvedValue(undefined),
    );

    await expect(service.deliver(liquidation)).resolves.toBe(true);
    expect(mockedAxios.post).toHaveBeenCalledTimes(1);
    const [url, body, options] = mockedAxios.post.mock.calls[0];
    expect(url).toBe('https://example.test/hooks');
    expect(JSON.parse(body as string)).toMatchObject({ type: 'liquidation.confirmed', data: liquidation });
    expect(options?.headers?.['x-stellarlend-webhook-signature']).toMatch(/^sha256=[a-f0-9]{64}$/);
    expect(options?.headers?.['x-stellarlend-webhook-id']).toBe('liquidation_tx-123_1710000000000');
  });

  it('fails closed when only one webhook setting is configured', async () => {
    const service = new LiquidationWebhookService(
      { url: 'https://example.test/hooks', secret: '', maxAttempts: 3, retryDelayMs: 1 },
      logger,
      mockedAxios,
    );

    await expect(service.deliver(liquidation)).resolves.toBe(false);
    expect(mockedAxios.post).not.toHaveBeenCalled();
  });

  it('retries transient failures with a bounded exponential backoff', async () => {
    mockedAxios.post
      .mockRejectedValueOnce({ response: { status: 503 } })
      .mockRejectedValueOnce({ response: { status: 429 } })
      .mockResolvedValueOnce({ status: 200 });
    const sleep = jest.fn().mockResolvedValue(undefined);
    mockedAxios.isAxiosError.mockReturnValue(true);
    const service = new LiquidationWebhookService(
      { url: 'https://example.test/hooks', secret: 'secret', maxAttempts: 3, retryDelayMs: 10 },
      logger,
      mockedAxios,
      sleep,
    );

    await expect(service.deliver(liquidation)).resolves.toBe(true);
    expect(mockedAxios.post).toHaveBeenCalledTimes(3);
    expect(sleep).toHaveBeenNthCalledWith(1, 10);
    expect(sleep).toHaveBeenNthCalledWith(2, 20);
  });

  it('does not retry client errors and stops at the attempt limit', async () => {
    mockedAxios.post.mockRejectedValue({ response: { status: 400 } });
    mockedAxios.isAxiosError.mockReturnValue(true);
    const service = new LiquidationWebhookService(
      { url: 'https://example.test/hooks', secret: 'secret', maxAttempts: 3, retryDelayMs: 1 },
      logger,
      mockedAxios,
      jest.fn().mockResolvedValue(undefined),
    );

    await expect(service.deliver(liquidation)).resolves.toBe(false);
    expect(mockedAxios.post).toHaveBeenCalledTimes(1);

    mockedAxios.post.mockReset();
    mockedAxios.post.mockRejectedValue({ response: { status: 500 } });
    await expect(service.deliver(liquidation)).resolves.toBe(false);
    expect(mockedAxios.post).toHaveBeenCalledTimes(3);
  });
});
