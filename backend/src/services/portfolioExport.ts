import { createHash } from 'crypto';

export interface PositionSummary {
  asset: string;
  supplied: string;
  borrowed: string;
  collateralFactor: number;
}

export interface BorrowerPortfolio {
  borrowerAddress: string;
  timestamp: number;
  totalSuppliedUsd: number;
  totalBorrowedUsd: number;
  healthFactor: number;
  positions: PositionSummary[];
}

export class PortfolioAttestationService {
  public generateAttestationPayload(portfolio: BorrowerPortfolio): { hash: string; signaturePayload: string } {
    const serialized = JSON.stringify({
      borrower: portfolio.borrowerAddress,
      timestamp: portfolio.timestamp,
      totalSupplied: portfolio.totalSuppliedUsd,
      totalBorrowed: portfolio.totalBorrowedUsd,
      healthFactor: portfolio.healthFactor,
      positions: portfolio.positions.map(p => ({
        asset: p.asset,
        supplied: p.supplied,
        borrowed: p.borrowed,
      })),
    });

    const hash = createHash('sha256').update(serialized).digest('hex');
    return {
      hash,
      signaturePayload: serialized,
    };
  }

  public exportPortfolioAsJson(portfolio: BorrowerPortfolio): string {
    return JSON.stringify(portfolio, null, 2);
  }
}
