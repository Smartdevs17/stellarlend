/**
 * Declarative rebalancing cron (Issue #1620).
 *
 * Schedules are owned by `bots/rebalance/rebalance.schedule.json`; the API job
 * only wires those definitions to the treasury rebalancer service. Production
 * attaches the exported processor to a BullMQ Worker on each schedule's
 * `targetQueue`; local/dev uses a single in-process interval.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import logger from '../utils/logger';
import { treasuryRebalancerService } from '../services/treasury-rebalancer.service';
import { runStablecoinPegCheck, STABLECOIN_PEG_JOB } from './stablecoinPeg.job';

export interface RebalanceScheduleEntry {
  name: string;
  enabled: boolean;
  cron: string;
  timezone: string;
  deviationThresholdPercent: number;
  slippageTolerancePercent: number;
  dryRun: boolean;
  targetQueue: string;
  assets?: string[];
}

export interface RebalanceSchedule {
  version: number;
  schedules: RebalanceScheduleEntry[];
}

const SCHEDULE_PATH = resolve(__dirname, '../../../bots/rebalance/rebalance.schedule.json');

export function readDeclarativeSchedule(): RebalanceSchedule {
  return JSON.parse(readFileSync(SCHEDULE_PATH, 'utf8')) as RebalanceSchedule;
}

export const DEFAULT_REBALANCE_SCHEDULE: RebalanceSchedule = readDeclarativeSchedule();

export interface RebalanceJobResult {
  name: string;
  triggered: boolean;
  simulatedActions: number;
  executed: boolean;
  timestamp: string;
  error?: string;
}

let schedulerHandle: ReturnType<typeof setInterval> | null = null;
let activeSchedule: RebalanceSchedule = DEFAULT_REBALANCE_SCHEDULE;

export function validateRebalanceSchedule(schedule: RebalanceSchedule): string[] {
  const errors: string[] = [];
  const names = new Set<string>();

  if (schedule.version !== 1) errors.push('schedule.version must be 1');
  for (const entry of schedule.schedules) {
    if (names.has(entry.name)) errors.push(`duplicate schedule name: ${entry.name}`);
    names.add(entry.name);
    if (entry.cron.trim().split(/\s+/).length !== 5) {
      errors.push(`invalid cron for ${entry.name}`);
    }
    if (entry.deviationThresholdPercent < 0 || entry.deviationThresholdPercent > 100) {
      errors.push(`deviationThresholdPercent out of range for ${entry.name}`);
    }
    if (entry.slippageTolerancePercent < 0 || entry.slippageTolerancePercent > 100) {
      errors.push(`slippageTolerancePercent out of range for ${entry.name}`);
    }
    if (entry.assets !== undefined && (!Array.isArray(entry.assets) || entry.assets.length === 0)) {
      errors.push(`assets must be a non-empty list for ${entry.name}`);
    }
    if (entry.name === STABLECOIN_PEG_JOB && !entry.assets?.length) {
      errors.push(`stablecoin-peg requires assets for ${entry.name}`);
    }
  }
  return errors;
}

export function loadRebalanceSchedule(schedule?: RebalanceSchedule): RebalanceSchedule {
  const selected = schedule ?? DEFAULT_REBALANCE_SCHEDULE;
  const errors = validateRebalanceSchedule(selected);
  if (errors.length > 0) throw new Error(`Invalid rebalance schedule: ${errors.join('; ')}`);
  activeSchedule = selected;
  return selected;
}

export function getRebalanceSchedule(): RebalanceSchedule {
  return structuredClone(activeSchedule);
}

export async function processRebalanceJob(name?: string): Promise<RebalanceJobResult[]> {
  const selected = activeSchedule.schedules.filter(
    (entry) => entry.enabled && (!name || entry.name === name)
  );
  if (name && selected.length === 0) throw new Error(`Unknown rebalance job: ${name}`);

  const results: RebalanceJobResult[] = [];
  for (const entry of selected) {
    const timestamp = new Date().toISOString();
    try {
      // The peg monitor reads prices and rebalances on depeg; it must not fall through
      // to the treasury drift logic below.
      if (entry.name === STABLECOIN_PEG_JOB) {
        const outcome = await runStablecoinPegCheck(entry);
        results.push({
          name: entry.name,
          triggered: outcome.triggered,
          simulatedActions: outcome.simulatedActions,
          executed: outcome.executed,
          timestamp,
        });
        continue;
      }

      const trigger = treasuryRebalancerService.checkRebalanceTrigger(
        entry.deviationThresholdPercent
      );
      const simulation = treasuryRebalancerService.simulateRebalance(
        entry.slippageTolerancePercent
      );
      const executed =
        trigger.shouldRebalance &&
        !entry.dryRun &&
        (await treasuryRebalancerService.executeRebalance(simulation).then(() => true));

      logger.info('Rebalance schedule executed', {
        job: entry.name,
        queue: entry.targetQueue,
        cron: entry.cron,
        dryRun: entry.dryRun,
        triggered: trigger.shouldRebalance,
        simulatedActions: simulation.actions.length,
        executed,
        timestamp,
      });

      results.push({
        name: entry.name,
        triggered: trigger.shouldRebalance,
        simulatedActions: simulation.actions.length,
        executed,
        timestamp,
      });
    } catch (error) {
      const message = (error as Error).message;
      logger.error('Rebalance schedule failed', {
        job: entry.name,
        queue: entry.targetQueue,
        error: message,
        timestamp,
      });
      results.push({
        name: entry.name,
        triggered: false,
        simulatedActions: 0,
        executed: false,
        timestamp,
        error: message,
      });
    }
  }
  return results;
}

export function startRebalanceCron(schedule?: RebalanceSchedule): void {
  if (schedule) loadRebalanceSchedule(schedule);
  if (schedulerHandle) return;

  logger.info('Rebalance cron started from declarative schedule', {
    jobs: activeSchedule.schedules.filter((entry) => entry.enabled).map((entry) => entry.name),
  });
  void processRebalanceJob();
  schedulerHandle = setInterval(() => void processRebalanceJob(), 60 * 60 * 1000);
}

export function stopRebalanceCron(): void {
  if (!schedulerHandle) return;
  clearInterval(schedulerHandle);
  schedulerHandle = null;
}
