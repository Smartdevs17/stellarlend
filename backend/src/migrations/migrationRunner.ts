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

      const startTime = Date.now();
      try {
        await step.up();
        const log: MigrationLog = {
          id: step.id,
          version: step.version,
          executedAt: new Date(),
          durationMs: Date.now() - startTime,
          status: 'SUCCESS',
        };
        this.executedHistory.set(step.id, log);
        results.push(log);
      } catch (err: any) {
        const failureLog: MigrationLog = {
          id: step.id,
          version: step.version,
          executedAt: new Date(),
          durationMs: Date.now() - startTime,
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
