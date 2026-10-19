import type { EventEmitter } from 'events';
import {
  collateralRatioMonitorService,
  PositionRiskData,
} from '../collateralRatioMonitor.service';
import { notificationEngine } from './notification.service';
import logger from '../../utils/logger';

type PositionUpdateSource = Pick<EventEmitter, 'on' | 'off'>;

/** Collateral value per unit, shown as the current price in the warning. */
function currentPrice(position: PositionRiskData): string {
  const amount = parseFloat(position.collateralAmount);
  const value = parseFloat(position.collateralValue);
  return amount > 0 && Number.isFinite(value) ? (value / amount).toFixed(4) : 'unknown';
}

/**
 * Sends the liquidation warning for one position: `health_factor_low` in the
 * danger band and `approaching_liquidation` once it is critical. Safe and
 * warning positions are ignored. The notification engine delivers the alert
 * to the user's subscribed channels (including Web Push) and rate-limits
 * repeats of the same alert.
 */
export async function sendLiquidationWarning(position: PositionRiskData): Promise<void> {
  const healthFactor = position.healthFactor.toFixed(2);
  const data = { asset: position.asset, riskLevel: position.riskLevel, healthFactor };

  if (position.riskLevel === 'danger') {
    await notificationEngine.sendAlert(
      position.address,
      'health_factor_low',
      {
        healthFactor,
        collateralValue: position.collateralValue,
        debtValue: position.debtValue,
      },
      data
    );
  } else if (position.riskLevel === 'critical') {
    await notificationEngine.sendAlert(
      position.address,
      'approaching_liquidation',
      {
        healthFactor,
        liquidationPrice: position.liquidationPrice,
        currentPrice: currentPrice(position),
      },
      data
    );
  }
}

/**
 * Subscribes to the collateral ratio monitor's position updates and turns
 * at-risk positions into liquidation warnings. Returns a function that stops
 * listening.
 */
export function startLiquidationWarnings(
  source: PositionUpdateSource = collateralRatioMonitorService
): () => void {
  const onPositionUpdate = (positions: PositionRiskData[]) => {
    for (const position of positions) {
      sendLiquidationWarning(position).catch((error) => {
        logger.error('Failed to send liquidation warning', { error, address: position.address });
      });
    }
  };

  source.on('position_update', onPositionUpdate);
  logger.info('Liquidation warning notifications started');
  return () => {
    source.off('position_update', onPositionUpdate);
  };
}
