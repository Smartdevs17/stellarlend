import { sandboxService } from '../services/sandbox.service';
import { apiKeyService } from '../services/apiKey.service';
import { webhookService } from '../services/webhook.service';

jest.mock('../utils/logger');
jest.mock('../services/auditLog.service', () => ({
  auditLogService: { record: jest.fn() },
}));

const originalEnv = process.env;

beforeEach(() => {
  process.env = { ...originalEnv, NODE_ENV: 'test' };
  delete process.env.SANDBOX_ENABLED;
  apiKeyService._reset();
  webhookService._reset();
  sandboxService._reset();
});

afterAll(() => {
  process.env = originalEnv;
});

describe('sandbox environment (#1377)', () => {
  it('is enabled outside production and reports in-memory isolation', () => {
    const status = sandboxService.status();
    expect(status.enabled).toBe(true);
    expect(status.isolation).toBe('in-memory');
    expect(status.resources).toEqual({ apiKeys: 0, webhookSubscriptions: 0 });
  });

  it('is disabled in production unless explicitly opted in', () => {
    process.env.NODE_ENV = 'production';
    expect(sandboxService.status().enabled).toBe(false);

    process.env.SANDBOX_ENABLED = 'true';
    expect(sandboxService.status().enabled).toBe(true);
  });

  it('reset isolates state and seeds a read-only demo key', async () => {
    await apiKeyService.create('prod-like', 'alice', ['admin']);
    webhookService.subscribe({ url: 'https://example.test/hook' });

    const result = await sandboxService.reset();

    expect(result.seededApiKey.rawKey).toHaveLength(64);
    expect(result.seededApiKey.record.scopes).toEqual(['read']);

    const keys = apiKeyService.list();
    expect(keys).toHaveLength(1);
    expect(keys[0].name).toBe('sandbox-demo-key');
    expect(webhookService.list()).toHaveLength(0);
    expect(sandboxService.status().resets).toBe(1);
    expect(sandboxService.status().lastResetAt).toBe(result.resetAt);
  });

  it('seeded key verifies and only carries the read scope', async () => {
    const { seededApiKey } = await sandboxService.reset();
    const verified = await apiKeyService.verify(seededApiKey.rawKey);

    expect(verified.valid).toBe(true);
    expect(verified.record?.scopes).toEqual(['read']);
    expect(apiKeyService.hasScope(verified.record, 'write')).toBe(false);
  });

  it('refuses reset when disabled', async () => {
    process.env.NODE_ENV = 'production';
    await expect(sandboxService.reset()).rejects.toThrow(/disabled/);
  });
});
