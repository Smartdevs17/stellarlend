export type RebalanceScheduleName =
  | "treasury-daily"
  | "pool-weekly"
  | "stablecoin-peg";

export interface RebalanceScheduleEntry {
  name: RebalanceScheduleName;
  enabled: boolean;
  cron: string;
  timezone: string;
  /**
   * For treasury and pool jobs: allocation drift that triggers a rebalance.
   * For `stablecoin-peg`: how far a stablecoin may sit from its $1 peg before
   * the job rebalances. The warning level is half of this value.
   */
  deviationThresholdPercent: number;
  slippageTolerancePercent: number;
  dryRun: boolean;
  targetQueue: string;
  /** Asset codes a job watches. Required for `stablecoin-peg`. */
  assets?: string[];
}

export interface RebalanceSchedule {
  version: number;
  schedules: RebalanceScheduleEntry[];
}

const DAILY_CRON = "0 0 * * *";
const WEEKLY_CRON = "0 0 * * 0";

export function parseCronExpression(expression: string): string[] {
  const fields = expression.trim().split(/\s+/);
  if (fields.length !== 5) {
    throw new Error(`Invalid cron expression: ${expression}`);
  }

  const ranges: ReadonlyArray<readonly [number, number]> = [
    [0, 59],
    [0, 23],
    [1, 31],
    [1, 12],
    [0, 7],
  ];

  for (let index = 0; index < fields.length; index += 1) {
    const field = fields[index];
    if (!field) throw new Error(`Invalid cron expression: ${expression}`);
    const values = field.split(",");
    for (const value of values) {
      const match = /^(\*|\d+)(?:-(\d+))?(?:\/(\d+))?$/.exec(value);
      if (!match) throw new Error(`Invalid cron expression: ${expression}`);
      const [, base, end, step] = match;
      const start = base === "*" ? (ranges[index]?.[0] ?? 0) : Number(base);
      const finish =
        end === undefined
          ? base === "*"
            ? (ranges[index]?.[1] ?? 0)
            : start
          : Number(end);
      const stepValue = step === undefined ? 1 : Number(step);
      const [minimum, maximum] = ranges[index] ?? [0, 0];
      if (
        !Number.isInteger(start) ||
        !Number.isInteger(finish) ||
        !Number.isInteger(stepValue) ||
        stepValue < 1 ||
        start < minimum ||
        finish > maximum ||
        finish < start
      ) {
        throw new Error(`Invalid cron expression: ${expression}`);
      }
    }
  }

  return fields;
}

export function validateSchedule(schedule: RebalanceSchedule): string[] {
  const errors: string[] = [];

  if (schedule.version !== 1) {
    errors.push("schedule.version must be 1");
  }

  const names = new Set<string>();
  for (const entry of schedule.schedules) {
    if (names.has(entry.name))
      errors.push(`duplicate schedule name: ${entry.name}`);
    names.add(entry.name);

    try {
      parseCronExpression(entry.cron);
    } catch (error) {
      errors.push(
        `invalid cron for ${entry.name}: ${(error as Error).message}`,
      );
    }

    if (!entry.timezone) errors.push(`missing timezone for ${entry.name}`);
    if (!entry.targetQueue)
      errors.push(`missing targetQueue for ${entry.name}`);
    if (
      entry.deviationThresholdPercent < 0 ||
      entry.deviationThresholdPercent > 100
    ) {
      errors.push(`deviationThresholdPercent out of range for ${entry.name}`);
    }
    if (
      entry.slippageTolerancePercent < 0 ||
      entry.slippageTolerancePercent > 100
    ) {
      errors.push(`slippageTolerancePercent out of range for ${entry.name}`);
    }
    if (entry.assets !== undefined) {
      if (
        !Array.isArray(entry.assets) ||
        entry.assets.length === 0 ||
        entry.assets.some((asset) => typeof asset !== "string" || asset.trim() === "")
      ) {
        errors.push(`assets must be a non-empty list of asset codes for ${entry.name}`);
      }
    }
    if (entry.name === "stablecoin-peg" && (entry.assets?.length ?? 0) === 0) {
      errors.push(`stablecoin-peg requires assets for ${entry.name}`);
    }
  }

  return errors;
}

export function enabledSchedules(
  schedule: RebalanceSchedule,
): RebalanceScheduleEntry[] {
  const errors = validateSchedule(schedule);
  if (errors.length > 0)
    throw new Error(`Invalid rebalance schedule: ${errors.join("; ")}`);
  return schedule.schedules.filter((entry) => entry.enabled);
}

export function loadSchedule(
  schedule: RebalanceSchedule = defaultRebalanceSchedule,
): RebalanceSchedule {
  const errors = validateSchedule(schedule);
  if (errors.length > 0)
    throw new Error(`Invalid rebalance schedule: ${errors.join("; ")}`);
  return schedule;
}

export const defaultRebalanceSchedule: RebalanceSchedule = {
  version: 1,
  schedules: [
    {
      name: "treasury-daily",
      enabled: true,
      cron: DAILY_CRON,
      timezone: "UTC",
      deviationThresholdPercent: 5,
      slippageTolerancePercent: 0.3,
      dryRun: false,
      targetQueue: "rebalance-treasury",
    },
    {
      name: "pool-weekly",
      enabled: true,
      cron: WEEKLY_CRON,
      timezone: "UTC",
      deviationThresholdPercent: 1,
      slippageTolerancePercent: 0.3,
      dryRun: false,
      targetQueue: "rebalance-pools",
    },
    {
      name: "stablecoin-peg",
      enabled: true,
      cron: "*/5 * * * *",
      timezone: "UTC",
      deviationThresholdPercent: 1,
      slippageTolerancePercent: 0.3,
      dryRun: false,
      targetQueue: "rebalance-stablecoin-peg",
      assets: ["USDC", "USDT"],
    },
  ],
};
