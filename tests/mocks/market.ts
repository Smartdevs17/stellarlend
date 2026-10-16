// SPDX-License-Identifier: Apache-2.0
import { BigNumber } from '@stellar/stellar-sdk';
import { VolatilityOracle } from '../../src/market/volatility';

/** Mock for testing volatility calculations */
export class MockVolatilityOracle extends VolatilityOracle {
  private volatility: BigNumber;

  constructor() {
    super(null, null);
    this.volatility = BigNumber.from('0');
  }

  setVolatility(value: BigNumber): void {
    this.volatility = value;
  }

  async getCurrentVolatility(): Promise<BigNumber> {
    return this.volatility;
  }
}