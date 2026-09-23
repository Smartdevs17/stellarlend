import { emergencyPauseService } from '../services/emergencyPause.service';

describe('EmergencyPauseService', () => {
  beforeEach(() => {
    emergencyPauseService.resume();
    emergencyPauseService.drainWithdrawalQueue();
  });

  it('queues withdrawals while paused', () => {
    emergencyPauseService.pause('manual');
    emergencyPauseService.queueWithdrawal({
      userAddress: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      amount: '10',
    });

    expect(emergencyPauseService.getWithdrawalQueue()).toHaveLength(1);
  });

  it('drains the queued withdrawals on resume flow', () => {
    emergencyPauseService.pause('manual');
    emergencyPauseService.queueWithdrawal({
      userAddress: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      amount: '10',
    });
    const drained = emergencyPauseService.drainWithdrawalQueue();
    emergencyPauseService.resume();

    expect(drained).toHaveLength(1);
    expect(emergencyPauseService.isPaused().paused).toBe(false);
  });

  it('rejects invalid limit updates (Issue #1090)', () => {
    expect(() => emergencyPauseService.updateLimits({ maxPerTransaction: -1 })).toThrow();
    expect(() => emergencyPauseService.updateLimits({ maxDailyPerUser: NaN })).toThrow();
    expect(() => emergencyPauseService.updateLimits({ maxDailyPoolDrain: Infinity })).toThrow();

    const updated = emergencyPauseService.updateLimits({ maxPerTransaction: 1000 });
    expect(updated.maxPerTransaction).toBe(1000);
    // Restore defaults for other tests
    emergencyPauseService.updateLimits({ maxPerTransaction: 500_000 });
  });
});
