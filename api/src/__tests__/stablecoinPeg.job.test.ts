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
  classifyPeg,
  PEG_REBALANCE_COOLDOWN_MS,
  PegJobEntry,
  resetStablecoinPegState,
  runStablecoinPegCheck,
} from '../jobs/stablecoinPeg.job';
import { loadRebalanceSchedule, processRebalanceJob } from '../jobs/rebalance.job';

const mockedService = treasuryRebalancerService as jest.Mocked<typeof treasuryRebalancerService>;

const pegEntry: PegJobEntry = {
  name: 'stablecoin-peg',
  dryRun: false,
  deviationThresholdPercent: 1,
  slippageTolerancePercent: 0.3,
  assets: ['USDC', 'USDT'],
  targetQueue: 'rebalance-stablecoin-peg',
};

const simulation = {
  actions: [
    {
      fromAsset: 'USDC',
      toAsset: 'USDT',
      amount: 10,
      slippageProtection: 0.3,
      estimatedGasCost: 0.1,
    },
  ],
  totalGasCost: 0.1,
  totalSlippage: 0.03,
  resultingAllocation: [],
  rebalanceDeviation: 1,
};

describe('classifyPeg', () => {
  it('classifies prices by distance from the $1 peg', () => {
    expect(classifyPeg(1, 1)).toMatchObject({ state: 'ok', deviationBps: 0 });
    expect(classifyPeg(0.996, 1)).toMatchObject({ state: 'ok' });
    // 75 bps is past the 50 bps warning level but short of the 100 bps depeg threshold.
    expect(classifyPeg(0.9925, 1)).toMatchObject({ state: 'warning' });
    expect(classifyPeg(0.985, 1)).toMatchObject({ state: 'depeg' });
    expect(classifyPeg(1.02, 1)).toMatchObject({ state: 'depeg' });
  });

  it('treats missing, non-finite, or non-positive prices as unknown', () => {
    expect(classifyPeg(undefined, 1).state).toBe('unknown');
    expect(classifyPeg(Number.NaN, 1).state).toBe('unknown');
    expect(classifyPeg(0, 1).state).toBe('unknown');
    expect(classifyPeg(-1, 1).state).toBe('unknown');
  });
});

describe('runStablecoinPegCheck', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    resetStablecoinPegState();
    mockedService.simulateRebalance.mockReturnValue(simulation);
    mockedService.executeRebalance.mockResolvedValue({
      executionId: 'test',
      timestamp: Date.now(),
      success: true,
      actions: [],
      totalCost: 0.1,
      totalSlippage: 0.03,
    });
  });

  it('stays quiet while every stablecoin is near its peg', async () => {
    mockedService.getAssetPrices.mockReturnValue({ USDC: 1.001, USDT: 0.999 });

    const outcome = await runStablecoinPegCheck(pegEntry, () => 0);

    expect(outcome).toMatchObject({ triggered: false, executed: false, simulatedActions: 0 });
    expect(mockedService.simulateRebalance).not.toHaveBeenCalled();
    expect(mockedService.executeRebalance).not.toHaveBeenCalled();
  });

  it('rebalances when a stablecoin reaches its depeg threshold', async () => {
    mockedService.getAssetPrices.mockReturnValue({ USDC: 1, USDT: 0.97 });

    const outcome = await runStablecoinPegCheck(pegEntry, () => 0);

    expect(outcome).toMatchObject({ triggered: true, executed: true, simulatedActions: 1 });
    expect(mockedService.simulateRebalance).toHaveBeenCalledWith(0.3);
    expect(mockedService.executeRebalance).toHaveBeenCalledTimes(1);
    expect(logger.error).toHaveBeenCalledWith(
      'Stablecoin depeg detected',
      expect.objectContaining({ job: 'stablecoin-peg' })
    );
  });

  it('suppresses repeat rebalances until the cooldown has elapsed', async () => {
    mockedService.getAssetPrices.mockReturnValue({ USDC: 0.97 });
    const entry = { ...pegEntry, assets: ['USDC'] };
    const start = 1_000_000;

    await runStablecoinPegCheck(entry, () => start);

    const inCooldown = await runStablecoinPegCheck(
      entry,
      () => start + PEG_REBALANCE_COOLDOWN_MS - 1
    );
    expect(inCooldown).toMatchObject({ triggered: true, executed: false });
    expect(mockedService.executeRebalance).toHaveBeenCalledTimes(1);

    const afterCooldown = await runStablecoinPegCheck(entry, () => start + PEG_REBALANCE_COOLDOWN_MS);
    expect(afterCooldown.executed).toBe(true);
    expect(mockedService.executeRebalance).toHaveBeenCalledTimes(2);
  });

  it('never executes a dry-run schedule', async () => {
    mockedService.getAssetPrices.mockReturnValue({ USDC: 0.9, USDT: 1 });

    const outcome = await runStablecoinPegCheck({ ...pegEntry, dryRun: true }, () => 0);

    expect(outcome).toMatchObject({ triggered: true, executed: false, simulatedActions: 1 });
    expect(mockedService.executeRebalance).not.toHaveBeenCalled();
  });

  it('reports an unavailable price as unknown and does not rebalance on it', async () => {
    mockedService.getAssetPrices.mockReturnValue({ USDC: 1 });

    const outcome = await runStablecoinPegCheck(pegEntry, () => 0);

    expect(outcome.readings.find((reading) => reading.asset === 'USDT')?.state).toBe('unknown');
    expect(outcome.executed).toBe(false);
    expect(logger.warn).toHaveBeenCalledWith(
      'Stablecoin price unavailable; peg check skipped for asset',
      expect.objectContaining({ asset: 'USDT' })
    );
  });

  it('refuses to run a peg entry that watches no assets', async () => {
    await expect(runStablecoinPegCheck({ ...pegEntry, assets: [] }, () => 0)).rejects.toThrow(
      /no assets/
    );
  });
});

describe('processRebalanceJob dispatch', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    resetStablecoinPegState();
    loadRebalanceSchedule();
  });

  it('routes the peg entry to the peg monitor instead of the treasury drift logic', async () => {
    mockedService.getAssetPrices.mockReturnValue({ USDC: 1, USDT: 1 });

    const [result] = await processRebalanceJob('stablecoin-peg');

    expect(result).toMatchObject({ name: 'stablecoin-peg', triggered: false, executed: false });
    expect(mockedService.getAssetPrices).toHaveBeenCalled();
    expect(mockedService.checkRebalanceTrigger).not.toHaveBeenCalled();
  });
});
