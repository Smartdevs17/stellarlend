import { Keypair, SorobanDataBuilder, xdr } from '@stellar/stellar-sdk';
import {
  DEFAULT_FEE_MARGIN_PERCENT,
  estimateFees,
  parseFeeMarginPercent,
  summarizeSorobanData,
} from '../services/sorobanFees';
import { ValidationError } from '../utils/errors';

function accountKey(): xdr.LedgerKey {
  return xdr.LedgerKey.account(
    new xdr.LedgerKeyAccount({ accountId: Keypair.random().xdrAccountId() })
  );
}

describe('sorobanFees', () => {
  describe('estimateFees', () => {
    it('adds the inclusion fee per operation to the resource fee and rounds the margin up', () => {
      const fees = estimateFees({ minResourceFee: '12345', operationCount: 1 });

      expect(fees).toEqual({
        baseFee: '100',
        operationCount: 1,
        inclusionFee: '100',
        resourceFee: '12345',
        totalFee: '12445',
        feeMarginPercent: DEFAULT_FEE_MARGIN_PERCENT,
        // 10% of 12445 is 1244.5, rounded up to 1245
        recommendedFee: '13690',
      });
    });

    it('charges the inclusion fee once per operation', () => {
      const fees = estimateFees({ minResourceFee: '0', operationCount: 3, feeMarginPercent: 0 });
      expect(fees.inclusionFee).toBe('300');
      expect(fees.totalFee).toBe('300');
      expect(fees.recommendedFee).toBe('300');
    });

    it('honours a custom base fee and a full margin', () => {
      const fees = estimateFees({
        minResourceFee: '1000',
        operationCount: 1,
        baseFee: '200',
        feeMarginPercent: 100,
      });
      expect(fees.totalFee).toBe('1200');
      expect(fees.recommendedFee).toBe('2400');
    });

    it('handles resource fees beyond the safe integer range', () => {
      const fees = estimateFees({
        minResourceFee: '9007199254740993',
        operationCount: 1,
        feeMarginPercent: 0,
      });
      expect(fees.totalFee).toBe('9007199254741093');
    });

    it.each([
      [{ minResourceFee: 'abc', operationCount: 1 }, 'minResourceFee'],
      [{ minResourceFee: '-5', operationCount: 1 }, 'minResourceFee'],
      [{ minResourceFee: '10', operationCount: 0 }, 'operationCount'],
      [{ minResourceFee: '10', operationCount: 1.5 }, 'operationCount'],
      [{ minResourceFee: '10', operationCount: 1, feeMarginPercent: 101 }, 'feeMarginPercent'],
      [{ minResourceFee: '10', operationCount: 1, feeMarginPercent: -1 }, 'feeMarginPercent'],
      [{ minResourceFee: '10', operationCount: 1, baseFee: '1.5' }, 'baseFee'],
    ])('rejects invalid input %j', (input, field) => {
      expect(() => estimateFees(input as never)).toThrow(ValidationError);
      expect(() => estimateFees(input as never)).toThrow(field);
    });
  });

  describe('parseFeeMarginPercent', () => {
    it('defaults when absent', () => {
      expect(parseFeeMarginPercent(undefined)).toBe(DEFAULT_FEE_MARGIN_PERCENT);
      expect(parseFeeMarginPercent(null)).toBe(DEFAULT_FEE_MARGIN_PERCENT);
      expect(parseFeeMarginPercent('')).toBe(DEFAULT_FEE_MARGIN_PERCENT);
    });

    it('accepts integers and integer strings within range', () => {
      expect(parseFeeMarginPercent(0)).toBe(0);
      expect(parseFeeMarginPercent('25')).toBe(25);
      expect(parseFeeMarginPercent(100)).toBe(100);
    });

    it.each(['12.5', 'ten', -1, 101, true, {}])('rejects %p', (raw) => {
      expect(() => parseFeeMarginPercent(raw)).toThrow(ValidationError);
    });
  });

  describe('summarizeSorobanData', () => {
    const builder = new SorobanDataBuilder()
      .setResources(150000, 2048, 512)
      .setResourceFee(98765)
      .setReadOnly([accountKey(), accountKey()])
      .setReadWrite([accountKey()]);

    const expected = {
      cpuInstructions: '150000',
      readBytes: '2048',
      writeBytes: '512',
      readOnlyEntries: 2,
      readWriteEntries: 1,
      resourceFee: '98765',
    };

    it('reads resources from a SorobanDataBuilder', () => {
      expect(summarizeSorobanData(builder)).toEqual(expected);
    });

    it('reads resources from the built XDR object and from base64', () => {
      const built = builder.build();
      expect(summarizeSorobanData(built)).toEqual(expected);
      expect(summarizeSorobanData(built.toXDR('base64'))).toEqual(expected);
    });

    it('returns null when no transaction data is present', () => {
      expect(summarizeSorobanData(undefined)).toBeNull();
      expect(summarizeSorobanData(null)).toBeNull();
      expect(summarizeSorobanData('')).toBeNull();
    });

    it('throws on malformed base64 data', () => {
      expect(() => summarizeSorobanData('not-xdr')).toThrow();
    });
  });
});
