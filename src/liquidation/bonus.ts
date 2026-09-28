// SPDX-License-Identifier: Apache-2.0
import { BigNumber } from '@stellar/stellar-sdk';
import { VolatilityOracle } from '../market/volatility';
import { LiquidationConfig } from '../config';

/**
 * Dynamic liquidation bonus calculator based on market volatility.
 * Bonus = baseBonus * (1 + volatilityFactor * volatilityScore)
 */
export class DynamicBonus {
  private static readonly BASE_BONUS = BigNumber.from('1.2'); // 20% base
  private static readonly MAX_VOLATILITY_FACTOR = BigNumber.from('0.8'); // 80% max scaling
  private static readonly VOLATILITY_THRESHOLDS = [
    BigNumber.from('0.05'), // Low volatility
    BigNumber.from('0.15'), // Moderate
    BigNumber.from('0.30')  // High
  ];

  constructor(
    private readonly config: LiquidationConfig,
    private readonly oracle: VolatilityOracle
  ) {}

  /** Calculate dynamic bonus based on current market volatility */
  async calculateBonus(): Promise<BigNumber> {
    const volatility = await this.oracle.getCurrentVolatility();
    const normalized = this.normalizeVolatility(volatility);
    const factor = this.calculateVolatilityFactor(normalized);
    return DynamicBonus.BASE_BONUS.plus(
      DynamicBonus.BASE_BONUS.multipliedBy(factor)
    );
  }

  private normalizeVolatility(volatility: BigNumber): BigNumber {
    if (volatility.lte(DynamicBonus.VOLATILITY_THRESHOLDS[0])) {
      return BigNumber.from('0'); // Stable market
    } else if (volatility.lte(DynamicBonus.VOLATILITY_THRESHOLDS[1])) {
      return volatility.dividedBy(DynamicBonus.VOLATILITY_THRESHOLDS[1]);
    } else if (volatility.lte(DynamicBonus.VOLATILITY_THRESHOLDS[2])) {
      return BigNumber.from('1').plus(
        volatility.minus(DynamicBonus.VOLATILITY_THRESHOLDS[1])
          .dividedBy(DynamicBonus.VOLATILITY_THRESHOLDS[2].minus(DynamicBonus.VOLATILITY_THRESHOLDS[1]))
      );
    }
    return BigNumber.from('2'); // Extreme volatility cap
  }

  private calculateVolatilityFactor(normalized: BigNumber): BigNumber {
    return DynamicBonus.MAX_VOLATILITY_FACTOR.multipliedBy(
      normalized.dividedBy(BigNumber.from('2'))
    );
  }
}