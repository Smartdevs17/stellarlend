import { apiKeyService } from './apiKey.service';
import { webhookService } from './webhook.service';

/**
 * Issue #1377 – API sandbox environment.
 *
 * Sandbox mode runs entirely against the in-memory stores used by the API
 * key and webhook services, so it never touches chain state or the shared
 * database. `reset()` wipes that state and re-seeds a documented demo key.
 *
 * Isolation model:
 *  - Sandbox data lives in the same process-local maps as the developer API,
 *    but sandbox endpoints are the only consumers that seed/reset them.
 *  - Production requires an explicit `SANDBOX_ENABLED=true` opt-in.
 */

export interface SandboxStatus {
  enabled: boolean;
  env: string;
  isolation: 'in-memory';
  resources: {
    apiKeys: number;
    webhookSubscriptions: number;
  };
  resets: number;
  lastResetAt?: string;
}

const startedAt = new Date().toISOString();
let resets = 0;
let lastResetAt: string | undefined;

function isEnabled(): boolean {
  return process.env.SANDBOX_ENABLED === 'true' || process.env.NODE_ENV !== 'production';
}

export interface SandboxResetResult {
  resetAt: string;
  seededApiKey: {
    rawKey: string;
    record: { id: string; name?: string; scopes: string[] };
  };
  note: string;
}

class SandboxService {
  status(): SandboxStatus {
    return {
      enabled: isEnabled(),
      env: process.env.NODE_ENV ?? 'development',
      isolation: 'in-memory',
      resources: {
        apiKeys: apiKeyService.list().length,
        webhookSubscriptions: webhookService.list().length,
      },
      resets,
      lastResetAt,
    };
  }

  /**
   * Reset sandbox state and seed a read-only demo key.
   * Throws when sandbox mode is not enabled (e.g. production without opt-in).
   */
  async reset(): Promise<SandboxResetResult> {
    if (!isEnabled()) {
      throw new Error(
        'Sandbox is disabled in this environment. Set SANDBOX_ENABLED=true to enable it.'
      );
    }

    apiKeyService._reset();
    webhookService._reset();

    const seeded = await apiKeyService.create('sandbox-demo-key', 'sandbox', ['read']);
    resets += 1;
    lastResetAt = new Date().toISOString();

    return {
      resetAt: lastResetAt,
      seededApiKey: {
        rawKey: seeded.rawKey,
        record: {
          id: seeded.record.id,
          name: seeded.record.name,
          scopes: seeded.record.scopes,
        },
      },
      note: 'Sandbox state reset. Raw key is shown once and only has the "read" scope.',
    };
  }

  startedAt(): string {
    return startedAt;
  }

  /** Exposed for testing only. */
  _reset(): void {
    resets = 0;
    lastResetAt = undefined;
  }
}

export const sandboxService = new SandboxService();
