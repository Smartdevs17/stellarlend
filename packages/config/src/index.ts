/**
 * @stellarlend/config — Issue #1602
 *
 * Single source of truth for all StellarLend service configuration.
 *
 * ## Problem being solved
 *
 * Before this package, every service (bots, api, oracle, scripts) duplicated
 * the same environment-variable reads and default values. When a variable name
 * changed (e.g. STELLAR_RPC_URL) or a new network was added, the change had to
 * be applied in four different files. That drift was the main cause of
 * "it works in my service but not yours" bugs.
 *
 * ## What lives here
 *
 * - `StellarNetworkConfig`  — RPC, Horizon, network ID, and fee settings
 * - `LogConfig`             — log level + format (shared across all services)
 * - `DatabaseConfig`        — Postgres primary/replica settings
 * - `BotSharedConfig`       — fields every bot needs (api base URL, dry-run flag)
 * - `loadStellarConfig()`   — read the above from process.env with defaults
 * - `loadLogConfig()`
 * - `loadDatabaseConfig()`
 * - `loadBotSharedConfig()`
 *
 * ## How to use
 *
 * ```ts
 * import { loadStellarConfig, loadLogConfig } from '@stellarlend/config';
 *
 * const stellar = loadStellarConfig();
 * const log     = loadLogConfig();
 * ```
 *
 * Service-specific fields that are not shared (e.g. `minProfitThresholdXlm`
 * on the liquidation bot) stay in the service's own config file and are
 * composed on top of the shared base.
 */

// ── Shared types ──────────────────────────────────────────────────────────────

export type LogLevel = 'debug' | 'info' | 'warn' | 'error' | 'silent';

export type StellarNetwork = 'mainnet' | 'testnet' | 'futurenet' | 'standalone';

/** Stellar / Soroban connection settings shared by every service. */
export interface StellarNetworkConfig {
  /** `mainnet` | `testnet` | `futurenet` | `standalone` */
  network: StellarNetwork;
  /** Soroban-RPC endpoint URL. */
  rpcUrl: string;
  /** Horizon REST API endpoint URL. */
  horizonUrl: string;
  /**
   * Maximum fee in stroops per operation.
   * Default: 100 000 (0.01 XLM).
   */
  maxFeeStroops: number;
  /** Network passphrase used to sign transactions. */
  networkPassphrase: string;
}

/** Structured logging configuration shared by all services. */
export interface LogConfig {
  level: LogLevel;
  /** `true` → emit newline-delimited JSON (production), `false` → pretty-print. */
  json: boolean;
}

/** Postgres primary + replicas. Used by api, oracle, indexer, scripts. */
export interface DatabaseConfig {
  primary: {
    host: string;
    port: number;
    database: string;
    user: string;
    password: string;
    maxConnections: number;
  };
  replicas: Array<{
    host: string;
    port: number;
    database: string;
    user: string;
    password: string;
    maxConnections: number;
    priority: number;
  }>;
  replication: {
    maxLagMs: number;
    healthCheckIntervalMs: number;
    failoverEnabled: boolean;
  };
}

/**
 * Fields shared by every bot (liquidation bot, rebalance bot, auction bidder).
 *
 * Bot-specific fields are added in each bot's own config and composed with
 * this interface via intersection types or `extends`.
 */
export interface BotSharedConfig {
  /** REST API base URL this bot talks to (e.g. `http://localhost:3000/api`). */
  apiBaseUrl: string;
  /** WebSocket URL for position health updates. */
  wsUrl: string;
  /** When `true` the bot evaluates but does not submit transactions. */
  dryRun: boolean;
  /** Minimum log level for this process. */
  logLevel: LogLevel;
}

// ── Loaders ───────────────────────────────────────────────────────────────────

function env(key: string, fallback: string): string {
  return process.env[key] ?? fallback;
}

function envInt(key: string, fallback: number): number {
  const v = process.env[key];
  if (v === undefined) return fallback;
  const n = parseInt(v, 10);
  return Number.isFinite(n) ? n : fallback;
}

function envBool(key: string, fallback: boolean): boolean {
  const v = process.env[key];
  if (v === undefined) return fallback;
  return v.toLowerCase() !== 'false' && v !== '0';
}

function envLogLevel(key: string, fallback: LogLevel): LogLevel {
  const v = process.env[key];
  const valid: LogLevel[] = ['debug', 'info', 'warn', 'error', 'silent'];
  return valid.includes(v as LogLevel) ? (v as LogLevel) : fallback;
}

/** Default network passphrase for each known network ID. */
const NETWORK_PASSPHRASES: Record<StellarNetwork, string> = {
  mainnet:    'Public Global Stellar Network ; September 2015',
  testnet:    'Test SDF Network ; September 2015',
  futurenet:  'Test SDF Future Network ; October 2022',
  standalone: 'Standalone Network ; February 2017',
};

/**
 * Load Stellar/Soroban connection settings from environment variables.
 *
 * | Variable                | Default                                   |
 * |-------------------------|-------------------------------------------|
 * | `STELLAR_NETWORK`       | `testnet`                                 |
 * | `STELLAR_RPC_URL`       | `https://soroban-testnet.stellar.org`     |
 * | `STELLAR_HORIZON_URL`   | `https://horizon-testnet.stellar.org`     |
 * | `STELLAR_MAX_FEE`       | `100000`                                  |
 * | `STELLAR_NETWORK_PASSPHRASE` | auto-derived from network if unset   |
 */
export function loadStellarConfig(): StellarNetworkConfig {
  const network = env('STELLAR_NETWORK', 'testnet') as StellarNetwork;
  return {
    network,
    rpcUrl:     env('STELLAR_RPC_URL',     'https://soroban-testnet.stellar.org'),
    horizonUrl: env('STELLAR_HORIZON_URL', 'https://horizon-testnet.stellar.org'),
    maxFeeStroops: envInt('STELLAR_MAX_FEE', 100_000),
    networkPassphrase: env(
      'STELLAR_NETWORK_PASSPHRASE',
      NETWORK_PASSPHRASES[network] ?? NETWORK_PASSPHRASES.testnet,
    ),
  };
}

/**
 * Load structured logging config.
 *
 * | Variable    | Default                                   |
 * |-------------|-------------------------------------------|
 * | `LOG_LEVEL` | `info`                                    |
 * | `LOG_JSON`  | `true` when `NODE_ENV=production`         |
 */
export function loadLogConfig(): LogConfig {
  return {
    level: envLogLevel('LOG_LEVEL', 'info'),
    json:  envBool('LOG_JSON', process.env.NODE_ENV === 'production'),
  };
}

/**
 * Load Postgres settings.
 *
 * | Variable               | Default       |
 * |------------------------|---------------|
 * | `DB_PRIMARY_HOST`      | `localhost`   |
 * | `DB_PRIMARY_PORT`      | `5432`        |
 * | `DB_NAME`              | `stellarlend` |
 * | `DB_USER`              | `postgres`    |
 * | `DB_PASSWORD`          | (empty)       |
 * | `DB_PRIMARY_MAX_CONN`  | `20`          |
 * | `DB_REPLICA1_HOST`     | `localhost`   |
 * | `DB_REPLICA1_PORT`     | `5433`        |
 * | `DB_REPLICA1_MAX_CONN` | `30`          |
 * | `DB_REPLICA2_HOST`     | `localhost`   |
 * | `DB_REPLICA2_PORT`     | `5434`        |
 * | `DB_REPLICA2_MAX_CONN` | `30`          |
 * | `DB_MAX_LAG_MS`        | `5000`        |
 * | `DB_HEALTH_CHECK_MS`   | `10000`       |
 * | `DB_FAILOVER_ENABLED`  | `true`        |
 */
export function loadDatabaseConfig(): DatabaseConfig {
  const dbName = env('DB_NAME', 'stellarlend');
  const dbUser = env('DB_USER', 'postgres');
  const dbPass = env('DB_PASSWORD', '');
  return {
    primary: {
      host:           env('DB_PRIMARY_HOST', 'localhost'),
      port:           envInt('DB_PRIMARY_PORT', 5432),
      database:       dbName,
      user:           dbUser,
      password:       dbPass,
      maxConnections: envInt('DB_PRIMARY_MAX_CONN', 20),
    },
    replicas: [
      {
        host:           env('DB_REPLICA1_HOST', 'localhost'),
        port:           envInt('DB_REPLICA1_PORT', 5433),
        database:       dbName,
        user:           dbUser,
        password:       dbPass,
        maxConnections: envInt('DB_REPLICA1_MAX_CONN', 30),
        priority: 1,
      },
      {
        host:           env('DB_REPLICA2_HOST', 'localhost'),
        port:           envInt('DB_REPLICA2_PORT', 5434),
        database:       dbName,
        user:           dbUser,
        password:       dbPass,
        maxConnections: envInt('DB_REPLICA2_MAX_CONN', 30),
        priority: 2,
      },
    ],
    replication: {
      maxLagMs:              envInt('DB_MAX_LAG_MS', 5_000),
      healthCheckIntervalMs: envInt('DB_HEALTH_CHECK_MS', 10_000),
      failoverEnabled:       envBool('DB_FAILOVER_ENABLED', true),
    },
  };
}

/**
 * Load the config fields that every bot shares.
 *
 * | Variable       | Default                              |
 * |----------------|--------------------------------------|
 * | `API_BASE_URL` | `http://localhost:3000/api`           |
 * | `WS_URL`       | `ws://localhost:3000/api/ws/health-updates` |
 * | `DRY_RUN`      | `true`                               |
 * | `LOG_LEVEL`    | `info`                               |
 */
export function loadBotSharedConfig(): BotSharedConfig {
  return {
    apiBaseUrl: env('API_BASE_URL', 'http://localhost:3000/api'),
    wsUrl:      env('WS_URL', 'ws://localhost:3000/api/ws/health-updates'),
    dryRun:     envBool('DRY_RUN', true),
    logLevel:   envLogLevel('LOG_LEVEL', 'info'),
  };
}

// ── Convenience re-exports ────────────────────────────────────────────────────

export { env, envInt, envBool, envLogLevel };
