/**
 * Stablecoin peg monitor (Issue #1534).
 *
 * Runs the `stablecoin-peg` entry of the declarative rebalance schedule. Each run
 * reads the latest price of every watched stablecoin, measures its distance from
 * the $1 peg, and rebalances when a stablecoin reaches its depeg threshold
 * (`deviationThresholdPercent`). A warning is logged at half the threshold.
 *
 * Safety properties:
 * - A missing, non-numeric, or non-positive price is reported as `unknown` and
 *   never triggers a rebalance.
 * - Automated rebalances are rate-limited by a per-job cooldown, so a price that
 *   oscillates around the threshold cannot cause repeated execution.
 * - `dryRun` schedules simulate and report, but never execute.
 */

import logger from '../utils/logger';
import { treasuryRebalancerService } from '../services/treasury-rebalancer.service';

/** Schedule name this monitor owns in `rebalance.schedule.json`. */
export const STABLECOIN_PEG_JOB = 'stablecoin-peg';

/** Target price of every watched stablecoin, in USD. */
export const PEG_TARGET_PRICE = 1;

/** Minimum time between automated depeg rebalances for one job. */
export const PEG_REBALANCE_COOLDOWN_MS = 15 * 60 * 1000;

export type PegState = 'ok' | 'warning' | 'depeg' | 'unknown';

/** The fields of a schedule entry that the peg monitor reads. */
export interface PegJobEntry {
  name: string;
  dryRun: boolean;
  deviationThresholdPercent: number;
  slippageTolerancePercent: number;
  assets?: string[];
  targetQueue: string;
}

export interface PegAssetReading {
  asset: string;
  price: number | undefined;
  deviationBps: number | undefined;
  state: PegState;
}

export interface PegCheckOutcome {
  triggered: boolean;
  simulatedActions: number;
  executed: boolean;
  readings: PegAssetReading[];
}

const lastRebalanceAt = new Map<string, number>();

/** Classifies one price against the peg. Returns `unknown` for unusable prices. */
export function classifyPeg(
  price: number | undefined,
  depegThresholdPercent: number,
): { state: PegState; deviationBps?: number } {
  if (price === undefined || !Number.isFinite(price) || price <= 0) {
    return { state: 'unknown' };
  }
  const deviationBps = (Math.abs(price - PEG_TARGET_PRICE) / PEG_TARGET_PRICE) * 10_000;
  const depegBps = depegThresholdPercent * 100;
  if (deviationBps >= depegBps) {
    return { state: 'depeg', deviationBps };
  }
  if (deviationBps >= depegBps / 2) {
    return { state: 'warning', deviationBps };
  }
  return { state: 'ok', deviationBps };
}

export function readPegs(
  prices: Record<string, number>,
  assets: string[],
  depegThresholdPercent: number,
): PegAssetReading[] {
  return assets.map((asset) => {
    const price = prices[asset];
    const classification = classifyPeg(price, depegThresholdPercent);
    return {
      asset,
      price,
      deviationBps: classification.deviationBps,
      state: classification.state,
    };
  });
}

/**
 * Runs one peg check for a `stablecoin-peg` schedule entry. Throws only if the
 * entry is misconfigured or the treasury service fails; the caller records that
 * as a failed run.
 */
export async function runStablecoinPegCheck(
  entry: PegJobEntry,
  now: () => number = Date.now,
): Promise<PegCheckOutcome> {
  const assets = entry.assets ?? [];
  if (assets.length === 0) {
    throw new Error(`stablecoin-peg job ${entry.name} has no assets to watch`);
  }

  const readings = readPegs(
    treasuryRebalancerService.getAssetPrices(),
    assets,
    entry.deviationThresholdPercent,
  );

  for (const reading of readings) {
    if (reading.state === 'unknown') {
      logger.warn('Stablecoin price unavailable; peg check skipped for asset', {
        job: entry.name,
        asset: reading.asset,
      });
    } else if (reading.state === 'warning') {
      logger.warn('Stablecoin approaching peg threshold', {
        job: entry.name,
        asset: reading.asset,
        price: reading.price,
        deviationBps: reading.deviationBps,
      });
    }
  }

  const depegged = readings.filter((reading) => reading.state === 'depeg');
  if (depegged.length === 0) {
    return { triggered: false, simulatedActions: 0, executed: false, readings };
  }

  logger.error('Stablecoin depeg detected', {
    job: entry.name,
    queue: entry.targetQueue,
    assets: depegged.map((reading) => ({
      asset: reading.asset,
      price: reading.price,
      deviationBps: reading.deviationBps,
    })),
  });

  const simulation = treasuryRebalancerService.simulateRebalance(entry.slippageTolerancePercent);
  const simulatedActions = simulation.actions.length;
  if (entry.dryRun || simulatedActions === 0) {
    return { triggered: true, simulatedActions, executed: false, readings };
  }

  const currentTime = now();
  const lastRun = lastRebalanceAt.get(entry.name);
  if (lastRun !== undefined && currentTime - lastRun < PEG_REBALANCE_COOLDOWN_MS) {
    logger.warn('Depeg rebalance suppressed by cooldown', {
      job: entry.name,
      remainingMs: PEG_REBALANCE_COOLDOWN_MS - (currentTime - lastRun),
    });
    return { triggered: true, simulatedActions, executed: false, readings };
  }

  await treasuryRebalancerService.executeRebalance(simulation);
  lastRebalanceAt.set(entry.name, currentTime);
  return { triggered: true, simulatedActions, executed: true, readings };
}

/** Test helper: clears the per-job cooldown record. */
export function resetStablecoinPegState(): void {
  lastRebalanceAt.clear();
}
