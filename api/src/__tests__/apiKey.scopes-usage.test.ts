import { apiKeyService, normalizeScopes } from '../services/apiKey.service';
import { requireScope, type AuthRequest } from '../middleware/auth';
import { ForbiddenError } from '../utils/errors';

jest.mock('../utils/logger');
jest.mock('../services/auditLog.service', () => ({
  auditLogService: { record: jest.fn() },
}));

beforeEach(() => {
  apiKeyService._reset();
});

describe('scoped API keys (#1374)', () => {
  it('defaults to wildcard scope for backwards compatibility', async () => {
    const { record } = await apiKeyService.create('legacy', 'alice');
    expect(record.scopes).toEqual(['*']);
  });

  it('stores requested scopes and exposes them without the hash', async () => {
    const { record, rawKey } = await apiKeyService.create('readonly', 'alice', ['read']);
    expect(record.scopes).toEqual(['read']);
    expect(JSON.stringify(apiKeyService.list())).not.toContain(rawKey);
    expect(JSON.stringify(apiKeyService.list())).toContain('"read"');
  });

  it('rejects unknown scopes', () => {
    expect(() => normalizeScopes(['drop-tables'])).toThrow(/Unknown scope/);
  });

  it('checks scopes including the wildcard', async () => {
    const { record: readonly } = await apiKeyService.create('a', 'alice', ['read']);
    const { record: wildcard } = await apiKeyService.create('b', 'alice');

    expect(apiKeyService.hasScope(readonly, 'read')).toBe(true);
    expect(apiKeyService.hasScope(readonly, 'write')).toBe(false);
    expect(apiKeyService.hasScope(wildcard, 'admin')).toBe(true);
    expect(apiKeyService.hasScope(undefined, 'read')).toBe(false);
  });

  it('preserves scopes when rotating', async () => {
    const { record } = await apiKeyService.create('scoped', 'alice', ['read', 'write']);
    const rotated = await apiKeyService.rotate(record.id, 'alice');
    expect(rotated.record.scopes).toEqual(['read', 'write']);
  });

  it('requireScope passes for granted scope and rejects missing scope', async () => {
    const { record: readonly } = await apiKeyService.create('a', 'alice', ['read']);
    const { record: writer } = await apiKeyService.create('b', 'alice', ['write']);

    const run = (scopes: any[]) => {
      const req = { apiKey: { id: 'k', scopes } } as AuthRequest;
      const next = jest.fn();
      try {
        requireScope('write')(req, {} as any, next);
      } catch (error) {
        return { error, next };
      }
      return { error: undefined, next };
    };

    expect(run(readonly.scopes).error).toBeInstanceOf(ForbiddenError);
    expect(run(writer.scopes).next).toHaveBeenCalled();
    expect(run(['*']).next).toHaveBeenCalled();
  });
});

describe('per-key usage analytics (#1375)', () => {
  it('counts requests and breakdowns per endpoint', async () => {
    const { record } = await apiKeyService.create('tracked', 'alice');

    apiKeyService.recordUsage(record.id, '/api/developer/usage');
    apiKeyService.recordUsage(record.id, '/api/developer/usage');
    apiKeyService.recordUsage(record.id, '/api/lending');

    const usage = apiKeyService.getUsage(record.id);
    expect(usage.requests).toBe(3);
    expect(usage.byEndpoint).toEqual({
      '/api/developer/usage': 2,
      '/api/lending': 1,
    });
    expect(usage.lastUsedAt).toBeDefined();
  });

  it('returns zeroed usage for unused keys', async () => {
    const { record } = await apiKeyService.create('unused', 'alice');
    expect(apiKeyService.getUsage(record.id)).toMatchObject({ requests: 0, byEndpoint: {} });
  });

  it('aggregates usage across keys for dashboards', async () => {
    const a = await apiKeyService.create('a', 'alice');
    const b = await apiKeyService.create('b', 'bob');
    apiKeyService.recordUsage(a.record.id, '/x');
    apiKeyService.recordUsage(b.record.id, '/y');

    expect(apiKeyService.listUsage()).toHaveLength(2);
  });
});
