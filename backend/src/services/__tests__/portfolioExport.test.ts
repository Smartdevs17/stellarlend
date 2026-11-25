import { describe, it, expect, beforeEach } from 'vitest';
import { PortfolioAttestationService, BorrowerPortfolio } from '../portfolioExport';

describe('PortfolioAttestationService', () => {
  let service: PortfolioAttestationService;
  let mockPortfolio: BorrowerPortfolio;

  beforeEach(() => {
    service = new PortfolioAttestationService();
    mockPortfolio = {
      borrowerAddress: 'GBORROWER123456789',
      timestamp: 1732520000,
      totalSuppliedUsd: 15000.5,
      totalBorrowedUsd: 5000.0,
      healthFactor: 2.15,
      positions: [
        { asset: 'XLM', supplied: '50000', borrowed: '0', collateralFactor: 0.75 },
        { asset: 'USDC', supplied: '0', borrowed: '5000', collateralFactor: 0.85 },
      ],
    };
  });

  it('generates consistent deterministic SHA256 hashes for portfolio payloads', () => {
    const payload1 = service.generateAttestationPayload(mockPortfolio);
    const payload2 = service.generateAttestationPayload(mockPortfolio);

    expect(payload1.hash).toBe(payload2.hash);
    expect(payload1.hash).toHaveLength(64);
  });

  it('exports valid formatted JSON', () => {
    const jsonStr = service.exportPortfolioAsJson(mockPortfolio);
    const parsed = JSON.parse(jsonStr);
    expect(parsed.borrowerAddress).toBe(mockPortfolio.borrowerAddress);
    expect(parsed.healthFactor).toBe(2.15);
  });
});
