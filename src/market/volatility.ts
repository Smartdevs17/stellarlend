// SPDX-License-Identifier: Apache-2.0
import { BigNumber } from '@stellar/stellar-sdk';
import { AssetPair } from '../types';
import { StellarClient } from '../client';

/**
 * Market volatility oracle using EMA of price deviations.
 * Measures % deviation from 7-day moving average.
 */
export class VolatilityOracle {
  private static readonly EMA_WINDOW = 15; // 15-minute intervals
  private static readonly EMA_ALPHA = 0.2; // Smoothing factor
  private priceHistory: BigNumber[] = [];
  private ema: BigNumber | null = null;

  constructor(private readonly client: StellarClient, private readonly asset: AssetPair) {}

  /** Get current volatility score (0 = stable, 2 = extreme) */
  async getCurrentVolatility(): Promise<BigNumber> {
    const currentPrice = await this.fetchCurrentPrice();
    const deviation = this.calculatePriceDeviation(currentPrice);
    return this.updateEMA(deviation).dividedBy(BigNumber.from('100')); // Convert to %
  }

  private async fetchCurrentPrice(): Promise<BigNumber> {
    const price = await this.client.getAssetPrice(this.asset);
    this.priceHistory.push(price);
    return price;
  }

  private calculatePriceDeviation(price: BigNumber): BigNumber {
    if (this.priceHistory.length < 2) return BigNumber.from('0');
    const oldest = this.priceHistory[0];
    return price.minus(oldest).abs().dividedBy(oldest).multipliedBy(BigNumber.from('100'));
  }

  private updateEMA(deviation: BigNumber): BigNumber {
    if (this.ema === null) {
      this.ema = deviation;
    } else {
      this.ema = this.ema.multipliedBy(BigNumber.from('1').minus(BigNumber.from(VolatilityOracle.EMA_ALPHA)))
        .plus(deviation.multipliedBy(BigNumber.from(VolatilityOracle.EMA_ALPHA)));
    }
    return this.ema;
  }
}