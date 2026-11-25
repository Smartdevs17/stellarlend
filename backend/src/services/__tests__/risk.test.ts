import { RiskService, VolatilityBucket, HAIRCUT_BPS } from '../risk';

describe('RiskService collateral haircuts', () => {
  let service: RiskService;

  beforeEach(() => {
    service = new RiskService();
    service.registerAsset({ assetSymbol: 'USDC', bucket: VolatilityBucket.LOW });
    service.registerAsset({ assetSymbol: 'XLM', bucket: VolatilityBucket.MEDIUM });
    service.registerAsset({ assetSymbol: 'MEME', bucket: VolatilityBucket.HIGH });
  });

  test('applies calibrated haircut for low volatility', () => {
    const raw = 1000000n;
    const discounted = service.applyHaircut('USDC', raw);
    expect(discounted).toBe(950000n);
  });

  test('applies calibrated haircut for medium volatility', () => {
    const raw = 1000000n;
    const discounted = service.applyHaircut('XLM', raw);
    expect(discounted).toBe(850000n);
  });

  test('falls back to extreme haircut for unrated assets', () => {
    const raw = 1000000n;
    const discounted = service.applyHaircut('UNKNOWN', raw);
    expect(discounted).toBe(500000n);
  });
});
