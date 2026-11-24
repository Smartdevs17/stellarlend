import logger from '../utils/logger';
import { treasuryRebalancerService } from '../services/treasury-rebalancer.service';

jest.mock('../utils/logger', () => ({
  __esModule: true,
  default: { info: jest.fn(), error: jest.fn(), warn: jest.fn(), debug: jest.fn() },
}));

jest.mock('../services/treasury-rebalancer.service', () => ({
  treasuryRebalancerService: {
    checkRebalanceTrigger: jest.fn(),
    simulateRebalance: jest.fn(),
    executeRebalance: jest.fn(),
    getAssetPrices: jest.fn(() => ({})),
  },
}));

import {
  DEFAULT_REBALANCE_SCHEDULE,
  getRebalanceSchedule,
  loadRebalanceSchedule,
  processRebalanceJob,
  readDeclarativeSchedule,
  validateRebalanceSchedule,
} from '../jobs/rebalance.job';

const mockedLogger = logger as jest.Mocked<typeof logger>;
const mockedService = treasuryRebalancerService as jest.Mocked<typeof treasuryRebalancerService>;

describe('declarative rebalance job', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    loadRebalanceSchedule();
    mockedService.checkRebalanceTrigger.mockReturnValue({
      shouldRebalance: true,
      maxDeviation: 10,
    });
    mockedService.simulateRebalance.mockReturnValue({
      actions: [
        {
          fromAsset: 'XLM',
          toAsset: 'USDC',
          amount: 10,
          slippageProtection: 0.3,
          estimatedGasCost: 0.1,
        },
      ],
      totalGasCost: 0.1,
      totalSlippage: 0.03,
      resultingAllocation: [],
      rebalanceDeviation: 10,
    });
    mockedService.executeRebalance.mockResolvedValue({
      executionId: 'test',
      timestamp: Date.now(),
      success: true,
      actions: [],
      totalCost: 0.1,
      totalSlippage: 0.03,
    });
  });

  it('loads the shared declarative schedule', () => {
    const schedule = readDeclarativeSchedule();
    expect(schedule.version).toBe(1);
    expect(schedule.schedules.map((entry) => entry.name)).toEqual([
      'treasury-daily',
      'pool-weekly',
      'stablecoin-peg',
    ]);
    expect(validateRebalanceSchedule(schedule)).toEqual([]);
    expect(getRebalanceSchedule()).toEqual(DEFAULT_REBALANCE_SCHEDULE);
  });

  it('runs a named job using schedule thresholds and emits structured logs', async () => {
    const results = await processRebalanceJob('treasury-daily');

    expect(results).toHaveLength(1);
    expect(results[0]).toMatchObject({
      name: 'treasury-daily',
      triggered: true,
      simulatedActions: 1,
      executed: true,
    });
    expect(mockedService.checkRebalanceTrigger).toHaveBeenCalledWith(5);
    expect(mockedService.simulateRebalance).toHaveBeenCalledWith(0.3);
    expect(mockedLogger.info).toHaveBeenCalledWith(
      'Rebalance schedule executed',
      expect.objectContaining({ job: 'treasury-daily', queue: 'rebalance-treasury' })
    );
  });

  it('does not execute dry-run jobs and rejects invalid schedules', async () => {
    const schedule = structuredClone(DEFAULT_REBALANCE_SCHEDULE);
    schedule.schedules = [schedule.schedules[0]!];
    schedule.schedules[0]!.dryRun = true;
    loadRebalanceSchedule(schedule);

    const [result] = await processRebalanceJob();
    expect(result?.executed).toBe(false);
    expect(mockedService.executeRebalance).not.toHaveBeenCalled();

    expect(() =>
      loadRebalanceSchedule({
        version: 2,
        schedules: [],
      })
    ).toThrow(/Invalid rebalance schedule/);
  });

  it('captures execution failures without throwing', async () => {
    mockedService.executeRebalance.mockRejectedValue(new Error('rpc down'));
    const [result] = await processRebalanceJob('pool-weekly');

    expect(result?.error).toBe('rpc down');
    expect(mockedLogger.error).toHaveBeenCalledWith(
      'Rebalance schedule failed',
      expect.objectContaining({ job: 'pool-weekly', error: 'rpc down' })
    );
  });
});
