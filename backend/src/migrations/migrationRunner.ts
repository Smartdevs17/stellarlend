import { systemClock, type Clock } from '../lib/clock';

export interface MigrationStep {
  id: string;
  name: string;
  version: number;
  up: () => Promise<void>;
  down: () => Promise<void>;
}

export interface MigrationLog {
  id: string;
  version: number;
  executedAt: Date;
  durationMs: number;
  status: 'SUCCESS' | 'FAILED';
  error?: string;
}

export class TypedMigrationRunner {
  private registeredMigrations: MigrationStep[] = [];
  private executedHistory: Map<string, MigrationLog> = new Map();

  constructor(private readonly clock: Clock = systemClock) {}

  public register(step: MigrationStep): void {
    if (this.registeredMigrations.some(m => m.id === step.id || m.version === step.version)) {
      throw new Error(`Duplicate migration ID or version detected: ${step.id} (v${step.version})`);
    }
    this.registeredMigrations.push(step);
    this.registeredMigrations.sort((a, b) => a.version - b.version);
  }

  public async runPending(): Promise<MigrationLog[]> {
    const results: MigrationLog[] = [];

    for (const step of this.registeredMigrations) {
      if (this.executedHistory.has(step.id)) continue;

      const startTime = this.clock.nowMilliseconds();
      try {
        await step.up();
        const log: MigrationLog = {
          id: step.id,
          version: step.version,
          executedAt: this.clock.nowDate(),
          durationMs: this.clock.nowMilliseconds() - startTime,
          status: 'SUCCESS',
        };
        this.executedHistory.set(step.id, log);
        results.push(log);
      } catch (err: any) {
        const failureLog: MigrationLog = {
          id: step.id,
          version: step.version,
          executedAt: this.clock.nowDate(),
          durationMs: this.clock.nowMilliseconds() - startTime,
          status: 'FAILED',
          error: err?.message || String(err),
        };
        this.executedHistory.set(step.id, failureLog);
        results.push(failureLog);
        throw new Error(`Migration ${step.id} failed: ${failureLog.error}`);
      }
    }

    return results;
  }

  public getExecutedLogs(): MigrationLog[] {
    return Array.from(this.executedHistory.values());
  }
}
