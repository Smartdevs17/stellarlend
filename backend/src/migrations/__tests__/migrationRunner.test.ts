import { describe, it, expect, beforeEach, vi } from 'vitest';
import { TypedMigrationRunner, MigrationStep } from '../migrationRunner';

describe('TypedMigrationRunner', () => {
  let runner: TypedMigrationRunner;

  beforeEach(() => {
    runner = new TypedMigrationRunner();
  });

  it('runs registered migrations in sequential version order', async () => {
    const order: number[] = [];
    const step1: MigrationStep = {
      id: '001_init',
      name: 'initial schema',
      version: 1,
      up: vi.fn().mockImplementation(async () => { order.push(1); }),
      down: vi.fn(),
    };
    const step2: MigrationStep = {
      id: '002_add_pools',
      name: 'add pools',
      version: 2,
      up: vi.fn().mockImplementation(async () => { order.push(2); }),
      down: vi.fn(),
    };

    runner.register(step2);
    runner.register(step1);

    const logs = await runner.runPending();
    expect(order).toEqual([1, 2]);
    expect(logs).toHaveLength(2);
    expect(logs[0].status).toBe('SUCCESS');
  });

  it('uses the injected clock for execution time and duration', async () => {
    let now = 1_700_000_000_000;
    runner = new TypedMigrationRunner({
      nowMilliseconds: () => now,
      nowSeconds: () => Math.floor(now / 1000),
      nowDate: () => new Date(now),
    });
    runner.register({
      id: '001_clock',
      name: 'clock',
      version: 1,
      up: async () => {
        now += 25;
      },
      down: vi.fn(),
    });

    const [log] = await runner.runPending();
    expect(log.durationMs).toBe(25);
    expect(log.executedAt.getTime()).toBe(now);
  });

  it('rejects duplicate versions', () => {
    runner.register({ id: 'a', name: 'a', version: 1, up: vi.fn(), down: vi.fn() });
    expect(() => {
      runner.register({ id: 'b', name: 'b', version: 1, up: vi.fn(), down: vi.fn() });
    }).toThrow('Duplicate');
  });
});
