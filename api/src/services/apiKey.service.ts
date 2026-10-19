import { createHash, randomBytes, timingSafeEqual } from 'crypto';
import bcrypt from 'bcryptjs';
import { randomUUID } from 'crypto';
import logger from '../utils/logger';
import { auditLogService } from './auditLog.service';
import { UnauthorizedError, ConflictError, NotFoundError } from '../utils/errors';

/**
 * Issue #387 – Secure API Key Hashing and Storage
 *
 * Design:
 *  - A raw key is generated once and returned to the caller. It is NEVER stored.
 *  - The first 8 characters form the "prefix" used to look up the key record
 *    without a full table scan.
 *  - The full key is hashed with bcrypt (cost 10) before persistence.
 *  - Comparison uses bcrypt.compare which is inherently timing-safe.
 *  - Key rotation creates a new key while the old one remains valid until
 *    explicitly revoked, giving callers an overlap window.
 *  - Revocation is immediate: the record is marked revoked and subsequent
 *    verifications reject it.
 */

const BCRYPT_COST = 10;
const KEY_BYTES = 32; // 256-bit raw key → 64-char hex string
const PREFIX_LENGTH = 8;

/**
 * Issue #1374 – Scoped API keys.
 *
 * Supported scopes. `*` grants full access and is the default so existing
 * keys keep working; callers can request narrow scopes for least privilege.
 */
export const API_SCOPES = ['read', 'write', 'admin', 'liquidation:read', 'webhooks:manage'] as const;
export type ApiScope = (typeof API_SCOPES)[number] | '*';
export const DEFAULT_SCOPES: ApiScope[] = ['*'];

export function normalizeScopes(scopes?: unknown): ApiScope[] {
  if (scopes === undefined || scopes === null) return [...DEFAULT_SCOPES];
  if (!Array.isArray(scopes) || scopes.length === 0) {
    throw new ConflictError('scopes must be a non-empty array');
  }
  const normalized = scopes.map((s) => String(s));
  for (const scope of normalized) {
    if (scope !== '*' && !(API_SCOPES as readonly string[]).includes(scope)) {
      throw new ConflictError(
        `Unknown scope "${scope}". Allowed: ${API_SCOPES.join(', ')}, *`
      );
    }
  }
  return normalized as ApiScope[];
}

export interface ApiKeyRecord {
  id: string;
  prefix: string;
  /** bcrypt hash – never the raw key */
  hash: string;
  name?: string;
  /** Issue #1374 – scopes granted to this key. `['*']` = full access. */
  scopes: ApiScope[];
  createdAt: string;
  lastUsedAt?: string;
  revokedAt?: string;
  createdBy?: string;
}

/**
 * Issue #1375 – Per-key usage analytics.
 */
export interface ApiKeyUsage {
  keyId: string;
  requests: number;
  byEndpoint: Record<string, number>;
  lastUsedAt?: string;
  firstUsedAt?: string;
}

const usageStore = new Map<string, ApiKeyUsage>();

const EMPTY_USAGE: Omit<ApiKeyUsage, 'keyId'> = {
  requests: 0,
  byEndpoint: {},
};

export interface CreateApiKeyResult {
  /** The raw key – shown ONCE, never stored. Caller must save it. */
  rawKey: string;
  record: Omit<ApiKeyRecord, 'hash'>;
}

export interface VerifyApiKeyResult {
  valid: boolean;
  record?: Omit<ApiKeyRecord, 'hash'>;
}

/**
 * In-memory store used when no external DB is configured.
 * In production, swap this for a DB-backed repository.
 */
const store = new Map<string, ApiKeyRecord>();

function generateRawKey(): string {
  return randomBytes(KEY_BYTES).toString('hex');
}

function extractPrefix(rawKey: string): string {
  return rawKey.slice(0, PREFIX_LENGTH);
}

/**
 * Redact a raw key for safe logging – shows prefix only.
 */
function redactKey(rawKey: string): string {
  return `${rawKey.slice(0, PREFIX_LENGTH)}${'*'.repeat(rawKey.length - PREFIX_LENGTH)}`;
}

class ApiKeyService {
  /**
   * Create a new API key.
   *
   * @param name    Human-readable label for the key.
   * @param actor   Address / identity of the creator (for audit log).
   * @returns       The raw key (shown once) and the stored record (no hash).
   */
  async create(name: string, actor: string, scopes?: ApiScope[]): Promise<CreateApiKeyResult> {
    const rawKey = generateRawKey();
    const prefix = extractPrefix(rawKey);
    const hash = await bcrypt.hash(rawKey, BCRYPT_COST);

    const record: ApiKeyRecord = {
      id: randomUUID(),
      prefix,
      hash,
      name,
      scopes: normalizeScopes(scopes),
      createdAt: new Date().toISOString(),
      createdBy: actor,
    };

    store.set(record.id, record);

    auditLogService.record({
      action: 'API_KEY_CREATED',
      actor,
      status: 'success',
      afterState: { keyId: record.id, prefix, name, scopes: record.scopes },
    });

    logger.info('API key created', { keyId: record.id, prefix, name, actor });

    const { hash: _omit, ...publicRecord } = record;
    return { rawKey, record: publicRecord };
  }

  /**
   * Verify a raw API key.
   *
   * Uses bcrypt.compare (constant-time) to prevent timing attacks.
   * Logs access (success or failure) for audit purposes.
   * Never logs the raw key value.
   */
  async verify(rawKey: string): Promise<VerifyApiKeyResult> {
    const prefix = extractPrefix(rawKey);

    // Find all records matching the prefix (avoids full scan)
    const candidates = [...store.values()].filter(
      (r) => r.prefix === prefix && !r.revokedAt
    );

    for (const record of candidates) {
      // bcrypt.compare is timing-safe by design
      const match = await bcrypt.compare(rawKey, record.hash);
      if (match) {
        // Update last-used timestamp
        record.lastUsedAt = new Date().toISOString();
        store.set(record.id, record);

        const { hash: _omit, ...publicRecord } = record;
        return { valid: true, record: publicRecord };
      }
    }

    // Log failed attempt without leaking the key
    logger.warn('API key verification failed', { prefix: redactKey(rawKey) });
    auditLogService.record({
      action: 'API_KEY_VERIFY_FAILED',
      actor: 'unknown',
      status: 'failed',
      beforeState: { prefix },
    });

    return { valid: false };
  }

  /**
   * Rotate an existing key.
   *
   * Creates a new key record. The old key remains valid until explicitly
   * revoked, giving callers an overlap window to update their configuration.
   *
   * @param keyId   ID of the key to rotate.
   * @param actor   Identity of the requester.
   */
  async rotate(keyId: string, actor: string): Promise<CreateApiKeyResult> {
    const existing = store.get(keyId);
    if (!existing) {
      throw new NotFoundError(`API key ${keyId} not found`);
    }
    if (existing.revokedAt) {
      throw new ConflictError(`API key ${keyId} is already revoked`);
    }

    // Create the replacement key (old key still valid until revoked)
    const result = await this.create(existing.name ?? 'rotated', actor, existing.scopes);

    auditLogService.record({
      action: 'API_KEY_ROTATED',
      actor,
      status: 'success',
      beforeState: { oldKeyId: keyId, oldPrefix: existing.prefix },
      afterState: { newKeyId: result.record.id, newPrefix: result.record.prefix },
    });

    logger.info('API key rotated', {
      oldKeyId: keyId,
      newKeyId: result.record.id,
      actor,
    });

    return result;
  }

  /**
   * Revoke a key immediately.
   *
   * After revocation, verify() will reject the key on the next call.
   */
  revoke(keyId: string, actor: string): void {
    const record = store.get(keyId);
    if (!record) {
      throw new NotFoundError(`API key ${keyId} not found`);
    }
    if (record.revokedAt) {
      throw new ConflictError(`API key ${keyId} is already revoked`);
    }

    record.revokedAt = new Date().toISOString();
    store.set(keyId, record);

    auditLogService.record({
      action: 'API_KEY_REVOKED',
      actor,
      status: 'success',
      beforeState: { keyId, prefix: record.prefix },
    });

    logger.info('API key revoked', { keyId, actor });
  }

  /**
   * List all keys (without hashes) for a given creator.
   */
  list(actor?: string): Omit<ApiKeyRecord, 'hash'>[] {
    return [...store.values()]
      .filter((r) => !actor || r.createdBy === actor)
      .map(({ hash: _omit, ...pub }) => pub);
  }

  /**
   * Migrate plaintext keys to bcrypt hashes.
   *
   * Accepts an array of { id, rawKey, ...metadata } objects representing
   * existing plaintext keys and re-hashes them in place.
   * Returns the number of keys migrated.
   */
  async migratePlaintextKeys(
    keys: Array<{ id: string; rawKey: string; name?: string; createdBy?: string }>
  ): Promise<number> {
    let migrated = 0;
    for (const k of keys) {
      const prefix = extractPrefix(k.rawKey);
      const hash = await bcrypt.hash(k.rawKey, BCRYPT_COST);
      const record: ApiKeyRecord = {
        id: k.id,
        prefix,
        hash,
        name: k.name,
        scopes: [...DEFAULT_SCOPES],
        createdAt: new Date().toISOString(),
        createdBy: k.createdBy,
      };
      store.set(record.id, record);
      migrated++;
    }

    logger.info('Plaintext API key migration complete', { migrated });
    auditLogService.record({
      action: 'API_KEY_MIGRATION',
      actor: 'system',
      status: 'success',
      afterState: { migrated },
    });

    return migrated;
  }

  /**
   * Issue #1374 – Check whether a key record grants a required scope.
   * `*` grants every scope.
   */
  hasScope(
    record: Pick<ApiKeyRecord, 'scopes'> | undefined,
    required: ApiScope
  ): boolean {
    if (!record) return false;
    return record.scopes.includes('*') || record.scopes.includes(required);
  }

  /**
   * Issue #1375 – Record one authenticated request for usage analytics.
   * Kept in-memory alongside the key store; endpoint is the matched route
   * path (or the raw path when no route matched).
   */
  recordUsage(keyId: string, endpoint: string): ApiKeyUsage {
    const now = new Date().toISOString();
    const existing = usageStore.get(keyId) ?? {
      keyId,
      ...EMPTY_USAGE,
      firstUsedAt: now,
    };
    existing.requests += 1;
    existing.byEndpoint[endpoint] = (existing.byEndpoint[endpoint] ?? 0) + 1;
    existing.lastUsedAt = now;
    usageStore.set(keyId, existing);
    return existing;
  }

  /**
   * Issue #1375 – Usage totals for a single key.
   * Unknown keys return a zeroed record rather than throwing so dashboards
   * can render keys that have not been used yet.
   */
  getUsage(keyId: string): ApiKeyUsage {
    return usageStore.get(keyId) ?? { keyId, ...EMPTY_USAGE, byEndpoint: {} };
  }

  /** Issue #1375 – All recorded usage, for aggregate dashboards. */
  listUsage(): ApiKeyUsage[] {
    return [...usageStore.values()];
  }

  /** Exposed for testing only. */
  _reset(): void {
    store.clear();
    usageStore.clear();
  }
}

export const apiKeyService = new ApiKeyService();
