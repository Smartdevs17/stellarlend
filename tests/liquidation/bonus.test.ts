// SPDX-License-Identifier: Apache-2.0
import { expect } from 'chai';
import { BigNumber } from '@stellar/stellar-sdk';
import { DynamicBonus } from '../../src/liquidation/bonus';
import { MockVolatilityOracle } from '../mocks/market';
import { LiquidationConfig } from '../../src/config';

describe('DynamicBonus', () => {
  const config = new LiquidationConfig();
  let oracle: MockVolatilityOracle;
  let bonus: DynamicBonus;

  beforeEach(() => {
    oracle = new MockVolatilityOracle();
    bonus = new DynamicBonus(config, oracle);
  });

  it('should return base bonus for 0% volatility', async () => {
    oracle.setVolatility(BigNumber.from('0'));
    const result = await bonus.calculateBonus();
    expect(result).to.equal(BigNumber.from('1.2'));
  });

  it('should scale bonus linearly up to 15% volatility', async () => {
    oracle.setVolatility(BigNumber.from('0.15')); // 15% → 1.2 * 1.4 = 1.68
    const result = await bonus.calculateBonus();
    expect(result).to.equal(BigNumber.from('1.68'));
  });

  it('should cap at 2x bonus for extreme volatility', async () => {
    oracle.setVolatility(BigNumber.from('0.5')); // >30% → capped
    const result = await bonus.calculateBonus();
    expect(result).to.equal(BigNumber.from('2.0'));
  });

  it('should handle floating-point precision', async () => {
    oracle.setVolatility(BigNumber.from('0.123456'));
    const result = await bonus.calculateBonus();
    expect(result.toString()).to.equal('1.5883648');
  });
});