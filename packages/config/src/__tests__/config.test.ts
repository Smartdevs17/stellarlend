/**
 * Issue #1602 — shared config package tests
 *
 * Tests that every loader reads from env vars and falls back to the documented
 * defaults.  No side effects: each test saves and restores process.env.
 */

import {
  loadStellarConfig,
  loadLogConfig,
  loadDatabaseConfig,
  loadBotSharedConfig,
  StellarNetworkConfig,
  LogConfig,
  DatabaseConfig,
  BotSharedConfig,
} from '../index';

function withEnv(overrides: Record<string, string | undefined>, fn: () => void) {
  const saved: Record<string, string | undefined> = {};
  for (const [k, v] of Object.entries(overrides)) {
    saved[k] = process.env[k];
    if (v === undefined) {
      delete process.env[k];
    } else {
      process.env[k] = v;
    }
  }
  try {
    fn();
  } finally {
    for (const [k, v] of Object.entries(saved)) {
      if (v === undefined) {
        delete process.env[k];
      } else {
        process.env[k] = v;
      }
    }
  }
}

// ── StellarNetworkConfig ──────────────────────────────────────────────────────

describe('loadStellarConfig', () => {
  it('returns testnet defaults when no env vars are set', () => {
    withEnv(
      {
        STELLAR_NETWORK: undefined,
        STELLAR_RPC_URL: undefined,
        STELLAR_HORIZON_URL: undefined,
        STELLAR_MAX_FEE: undefined,
        STELLAR_NETWORK_PASSPHRASE: undefined,
      },
      () => {
        const cfg: StellarNetworkConfig = loadStellarConfig();
        expect(cfg.network).toBe('testnet');
        expect(cfg.rpcUrl).toBe('https://soroban-testnet.stellar.org');
        expect(cfg.horizonUrl).toBe('https://horizon-testnet.stellar.org');
        expect(cfg.maxFeeStroops).toBe(100_000);
        expect(cfg.networkPassphrase).toBe('Test SDF Network ; September 2015');
      },
    );
  });

  it('reads STELLAR_NETWORK=mainnet and derives the mainnet passphrase', () => {
    withEnv(
      {
        STELLAR_NETWORK: 'mainnet',
        STELLAR_NETWORK_PASSPHRASE: undefined,
      },
      () => {
        const cfg = loadStellarConfig();
        expect(cfg.network).toBe('mainnet');
        expect(cfg.networkPassphrase).toBe('Public Global Stellar Network ; September 2015');
      },
    );
  });

  it('honours an explicit STELLAR_NETWORK_PASSPHRASE override', () => {
    withEnv(
      { STELLAR_NETWORK: 'standalone', STELLAR_NETWORK_PASSPHRASE: 'My Custom Net' },
      () => {
        const cfg = loadStellarConfig();
        expect(cfg.networkPassphrase).toBe('My Custom Net');
      },
    );
  });

  it('parses STELLAR_MAX_FEE as integer', () => {
    withEnv({ STELLAR_MAX_FEE: '500000' }, () => {
      expect(loadStellarConfig().maxFeeStroops).toBe(500_000);
    });
  });

  it('falls back to default when STELLAR_MAX_FEE is non-numeric', () => {
    withEnv({ STELLAR_MAX_FEE: 'not-a-number' }, () => {
      expect(loadStellarConfig().maxFeeStroops).toBe(100_000);
    });
  });
});

// ── LogConfig ─────────────────────────────────────────────────────────────────

describe('loadLogConfig', () => {
  it('defaults to info level, non-JSON outside production', () => {
    withEnv({ LOG_LEVEL: undefined, LOG_JSON: undefined, NODE_ENV: 'development' }, () => {
      const cfg: LogConfig = loadLogConfig();
      expect(cfg.level).toBe('info');
      expect(cfg.json).toBe(false);
    });
  });

  it('enables JSON in production when LOG_JSON not set', () => {
    withEnv({ NODE_ENV: 'production', LOG_JSON: undefined }, () => {
      expect(loadLogConfig().json).toBe(true);
    });
  });

  it('respects explicit LOG_JSON=false in production', () => {
    withEnv({ NODE_ENV: 'production', LOG_JSON: 'false' }, () => {
      expect(loadLogConfig().json).toBe(false);
    });
  });

  it('accepts all valid log levels', () => {
    for (const level of ['debug', 'info', 'warn', 'error', 'silent'] as const) {
      withEnv({ LOG_LEVEL: level }, () => {
        expect(loadLogConfig().level).toBe(level);
      });
    }
  });

  it('falls back to info for an unknown level', () => {
    withEnv({ LOG_LEVEL: 'verbose' }, () => {
      expect(loadLogConfig().level).toBe('info');
    });
  });
});

// ── DatabaseConfig ────────────────────────────────────────────────────────────

describe('loadDatabaseConfig', () => {
  it('returns localhost defaults', () => {
    withEnv(
      {
        DB_PRIMARY_HOST: undefined,
        DB_NAME: undefined,
        DB_USER: undefined,
        DB_PASSWORD: undefined,
      },
      () => {
        const cfg: DatabaseConfig = loadDatabaseConfig();
        expect(cfg.primary.host).toBe('localhost');
        expect(cfg.primary.port).toBe(5432);
        expect(cfg.primary.database).toBe('stellarlend');
        expect(cfg.primary.user).toBe('postgres');
        expect(cfg.primary.maxConnections).toBe(20);
      },
    );
  });

  it('reads custom primary settings', () => {
    withEnv(
      {
        DB_PRIMARY_HOST: 'db.internal',
        DB_PRIMARY_PORT: '5431',
        DB_NAME: 'prod_db',
        DB_PRIMARY_MAX_CONN: '50',
      },
      () => {
        const cfg = loadDatabaseConfig();
        expect(cfg.primary.host).toBe('db.internal');
        expect(cfg.primary.port).toBe(5431);
        expect(cfg.primary.database).toBe('prod_db');
        expect(cfg.primary.maxConnections).toBe(50);
      },
    );
  });

  it('provides two replicas by default', () => {
    withEnv({}, () => {
      const cfg = loadDatabaseConfig();
      expect(cfg.replicas).toHaveLength(2);
      expect(cfg.replicas[0]!.priority).toBe(1);
      expect(cfg.replicas[1]!.priority).toBe(2);
    });
  });

  it('applies correct replication defaults', () => {
    withEnv(
      {
        DB_MAX_LAG_MS: undefined,
        DB_HEALTH_CHECK_MS: undefined,
        DB_FAILOVER_ENABLED: undefined,
      },
      () => {
        const cfg = loadDatabaseConfig();
        expect(cfg.replication.maxLagMs).toBe(5_000);
        expect(cfg.replication.healthCheckIntervalMs).toBe(10_000);
        expect(cfg.replication.failoverEnabled).toBe(true);
      },
    );
  });

  it('can disable failover', () => {
    withEnv({ DB_FAILOVER_ENABLED: 'false' }, () => {
      expect(loadDatabaseConfig().replication.failoverEnabled).toBe(false);
    });
  });
});

// ── BotSharedConfig ───────────────────────────────────────────────────────────

describe('loadBotSharedConfig', () => {
  it('returns localhost defaults', () => {
    withEnv({ API_BASE_URL: undefined, WS_URL: undefined, DRY_RUN: undefined }, () => {
      const cfg: BotSharedConfig = loadBotSharedConfig();
      expect(cfg.apiBaseUrl).toBe('http://localhost:3000/api');
      expect(cfg.wsUrl).toBe('ws://localhost:3000/api/ws/health-updates');
      expect(cfg.dryRun).toBe(true);
      expect(cfg.logLevel).toBe('info');
    });
  });

  it('reads custom API_BASE_URL', () => {
    withEnv({ API_BASE_URL: 'https://api.example.com' }, () => {
      expect(loadBotSharedConfig().apiBaseUrl).toBe('https://api.example.com');
    });
  });

  it('sets dryRun=false when DRY_RUN=false', () => {
    withEnv({ DRY_RUN: 'false' }, () => {
      expect(loadBotSharedConfig().dryRun).toBe(false);
    });
  });
});
