import { describe, it, expect, beforeEach } from 'vitest';
import { OracleStalenessValidator } from '../oracleValidator';

describe('OracleStalenessValidator', () => {
  let validator: OracleStalenessValidator;

  beforeEach(() => {
    validator = new OracleStalenessValidator(300); // 5 min
  });

  it('accepts fresh valid prices', () => {
    const record = {
      assetSymbol: 'XLM',
      priceUsd: 0.28,
      publishedAt: Date.now() - 30000, // 30 sec old
    };
    expect(validator.validatePriceRecord(record)).toBe(0.28);
  });

  it('rejects stale prices exceeding maximum allowable age', () => {
    const record = {
      assetSymbol: 'XLM',
      priceUsd: 0.28,
      publishedAt: Date.now() - 400000, // > 6 min old
    };
    expect(() => validator.validatePriceRecord(record)).toThrow('is stale');
  });

  it('rejects non-positive prices', () => {
    const record = {
      assetSymbol: 'XLM',
      priceUsd: 0,
      publishedAt: Date.now(),
    };
    expect(() => validator.validatePriceRecord(record)).toThrow('Invalid non-positive');
  });
});
