import { describe, it, expect, beforeEach } from 'vitest';
import { EgressAllowlist, EgressSecurityError } from '../egressAllowlist';

describe('EgressAllowlist', () => {
  let allowlist: EgressAllowlist;

  beforeEach(() => {
    allowlist = new EgressAllowlist([
      'horizon.stellar.org',
      'horizon-testnet.stellar.org',
      '*.stellar.org',
      'api.coingecko.com',
    ]);
  });

  it('permits authorized domains', () => {
    expect(() => allowlist.validateUrl('https://horizon.stellar.org/fee_stats')).not.toThrow();
    expect(() => allowlist.validateUrl('https://api.coingecko.com/api/v3/ping')).not.toThrow();
    expect(() => allowlist.validateUrl('https://soroban-rpc.stellar.org/')).not.toThrow();
  });

  it('blocks unlisted domains', () => {
    expect(() => allowlist.validateUrl('https://evil-hacker.com/steal')).toThrow(EgressSecurityError);
  });

  it('blocks SSRF to cloud metadata and localhost', () => {
    expect(() => allowlist.validateUrl('https://169.254.169.254/latest/meta-data/')).toThrow('Restricted internal address');
    expect(() => allowlist.validateUrl('https://localhost:8080/admin')).toThrow('Restricted internal address');
  });

  it('rejects insecure protocols like http and ftp', () => {
    expect(() => allowlist.validateUrl('http://horizon.stellar.org/')).toThrow('Blocked outbound egress protocol');
  });
});
