import { EventEmitter } from 'events';

jest.mock('../services/collateralRatioMonitor.service', () => ({
  collateralRatioMonitorService: { on: jest.fn(), off: jest.fn() },
}));

import type { PositionRiskData } from '../services/collateralRatioMonitor.service';
import { notificationEngine } from '../services/notification-engine/notification.service';
import {
  sendLiquidationWarning,
  startLiquidationWarnings,
} from '../services/notification-engine/liquidationWarnings';

function position(overrides: Partial<PositionRiskData> = {}): PositionRiskData {
  return {
    address: 'GUSER',
    asset: 'XLM',
    collateralAmount: '1000',
    debtAmount: '800',
    collateralValue: '2500',
    debtValue: '2000',
    currentRatio: 12500,
    requiredRatio: 11000,
    healthFactor: 1.3,
    riskLevel: 'danger',
    liquidationPrice: '0.2250',
    timestamp: 0,
    ...overrides,
  };
}

let sendAlert: jest.SpyInstance;

beforeEach(() => {
  sendAlert = jest.spyOn(notificationEngine, 'sendAlert').mockResolvedValue([]);
});

afterEach(() => {
  sendAlert.mockRestore();
});

describe('sendLiquidationWarning', () => {
  it('sends health_factor_low for a position in the danger band', async () => {
    await sendLiquidationWarning(position());

    expect(sendAlert).toHaveBeenCalledWith(
      'GUSER',
      'health_factor_low',
      { healthFactor: '1.30', collateralValue: '2500', debtValue: '2000' },
      { asset: 'XLM', riskLevel: 'danger', healthFactor: '1.30' }
    );
  });

  it('sends approaching_liquidation once the position is critical', async () => {
    await sendLiquidationWarning(position({ healthFactor: 1.04, riskLevel: 'critical' }));

    expect(sendAlert).toHaveBeenCalledWith(
      'GUSER',
      'approaching_liquidation',
      { healthFactor: '1.04', liquidationPrice: '0.2250', currentPrice: '2.5000' },
      { asset: 'XLM', riskLevel: 'critical', healthFactor: '1.04' }
    );
  });

  it('ignores safe and warning positions', async () => {
    await sendLiquidationWarning(position({ healthFactor: 2.4, riskLevel: 'safe' }));
    await sendLiquidationWarning(position({ healthFactor: 1.7, riskLevel: 'warning' }));

    expect(sendAlert).not.toHaveBeenCalled();
  });
});

describe('startLiquidationWarnings', () => {
  it('warns on at-risk position updates until stopped', () => {
    const monitor = new EventEmitter();
    const stop = startLiquidationWarnings(monitor);

    monitor.emit('position_update', [
      position(),
      position({ address: 'GSAFE', healthFactor: 2.5, riskLevel: 'safe' }),
    ]);
    expect(sendAlert).toHaveBeenCalledTimes(1);
    expect(sendAlert.mock.calls[0]![0]).toBe('GUSER');

    stop();
    monitor.emit('position_update', [position()]);
    expect(sendAlert).toHaveBeenCalledTimes(1);
  });
});
