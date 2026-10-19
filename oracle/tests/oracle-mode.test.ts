/**
 * Issue #1370 – ORACLE_MODE configuration.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { loadConfig } from '../src/config.js';

describe('Oracle mode configuration', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv };
    process.env.STELLAR_NETWORK = 'testnet';
    process.env.STELLAR_RPC_URL = 'https://soroban-testnet.stellar.org';
    process.env.CONTRACT_ID = 'CTEST123456789';
    process.env.ADMIN_SECRET_KEY = 'STEST123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ123456789';
    delete process.env.ORACLE_MODE;
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it('defaults to push mode (backwards compatible)', () => {
    expect(loadConfig().oracleMode).toBe('push');
  });

  it('accepts pull and both', () => {
    process.env.ORACLE_MODE = 'pull';
    expect(loadConfig().oracleMode).toBe('pull');

    process.env.ORACLE_MODE = 'both';
    expect(loadConfig().oracleMode).toBe('both');
  });

  it('rejects an unknown mode', () => {
    process.env.ORACLE_MODE = 'sideways';
    expect(() => loadConfig()).toThrow();
  });
});
