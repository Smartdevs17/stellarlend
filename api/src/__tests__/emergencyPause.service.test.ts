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

  it('pauses a single asset without pausing the whole service', () => {
    emergencyPauseService.pauseAsset('USDC', 'manual');

    expect(emergencyPauseService.isAssetPaused('USDC').paused).toBe(true);
    expect(emergencyPauseService.isAssetPaused('XLM').paused).toBe(false);
    expect(emergencyPauseService.isPaused().paused).toBe(false);
  });

  it('queues withdrawals for a paused asset only', () => {
    emergencyPauseService.pauseAsset('USDC', 'manual');
    emergencyPauseService.queueWithdrawal({
      userAddress: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      amount: '10',
      asset: 'USDC',
    });
    emergencyPauseService.queueWithdrawal({
      userAddress: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      amount: '5',
      asset: 'XLM',
    });

    expect(emergencyPauseService.getWithdrawalQueue()).toHaveLength(1);
    expect(emergencyPauseService.getWithdrawalQueue()[0].asset).toBe('USDC');
  });

  it('reversibly resumes a single asset', () => {
    emergencyPauseService.pauseAsset('USDC', 'manual');
    emergencyPauseService.resumeAsset('USDC');

    expect(emergencyPauseService.isAssetPaused('USDC').paused).toBe(false);
    expect(emergencyPauseService.isPaused().paused).toBe(false);
  });

  it('drains queued withdrawals for a resumed asset', () => {
    emergencyPauseService.pauseAsset('USDC', 'manual');
    emergencyPauseService.queueWithdrawal({
      userAddress: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      amount: '10',
      asset: 'USDC',
    });
    emergencyPauseService.resumeAsset('USDC');
    const drained = emergencyPauseService.drainWithdrawalQueue();

    expect(drained).toHaveLength(1);
    expect(emergencyPauseService.isAssetPaused('USDC').paused).toBe(false);
  });
});
