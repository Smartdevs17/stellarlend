import { describe, it, expect, beforeEach } from 'vitest';
import { OracleStalenessValidator } from '../oracleValidator';

describe('OracleStalenessValidator', () => {
  let validator: OracleStalenessValidator;
  const now = 1_700_000_000_000;

  beforeEach(() => {
    validator = new OracleStalenessValidator(300, {
      nowMilliseconds: () => now,
      nowSeconds: () => Math.floor(now / 1000),
      nowDate: () => new Date(now),
    }); // 5 min
  });

  it('accepts fresh valid prices', () => {
    const record = {
      assetSymbol: 'XLM',
      priceUsd: 0.28,
      publishedAt: now - 30000, // 30 sec old
    };
    expect(validator.validatePriceRecord(record)).toBe(0.28);
  });

  it('rejects stale prices exceeding maximum allowable age', () => {
    const record = {
      assetSymbol: 'XLM',
      priceUsd: 0.28,
      publishedAt: now - 400000, // > 6 min old
    };
    expect(() => validator.validatePriceRecord(record)).toThrow('is stale');
  });

  it('rejects non-positive prices', () => {
    const record = {
      assetSymbol: 'XLM',
      priceUsd: 0,
      publishedAt: now,
    };
    expect(() => validator.validatePriceRecord(record)).toThrow('Invalid non-positive');
  });
});
