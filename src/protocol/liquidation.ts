// SPDX-License-Identifier: Apache-2.0
import { BigNumber } from '@stellar/stellar-sdk';
import { DynamicBonus } from '../liquidation/bonus';
import { LiquidationEvent } from '../types';

/**
 * Liquidation protocol with dynamic bonus support.
 */
export class LiquidationProtocol {
  constructor(private readonly bonusCalculator: DynamicBonus) {}

  /**
   * Execute liquidation with dynamic bonus applied.
   * @returns {Promise<BigNumber>} Total collateral seized (including bonus)
   */
  async execute(event: LiquidationEvent): Promise<BigNumber> {
    const baseCollateral = this.calculateBaseCollateral(event);
    const bonus = await this.bonusCalculator.calculateBonus();
    const totalCollateral = baseCollateral.multipliedBy(bonus);
    return this.seizeCollateral(event, totalCollateral);
  }

  private calculateBaseCollateral(event: LiquidationEvent): BigNumber {
    // Existing logic unchanged
    return event.loanAmount.multipliedBy(event.liquidationThreshold);
  }

  private async seizeCollateral(
    event: LiquidationEvent,
    amount: BigNumber
  ): Promise<BigNumber> {
    // Existing logic unchanged
    return amount;
  }
}