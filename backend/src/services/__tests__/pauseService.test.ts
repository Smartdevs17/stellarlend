import { describe, it, expect, beforeEach } from 'vitest';
import { TimelockedPauseService } from '../pauseService';

describe('TimelockedPauseService', () => {
  let service: TimelockedPauseService;

  beforeEach(() => {
    service = new TimelockedPauseService(3600, 2);
  });

  it('creates proposal with future execution timestamp', () => {
    const prop = service.propose('POOL_A', 'G_ADMIN_1');
    expect(prop.executableAt).toBeGreaterThan(prop.proposedAt);
    expect(prop.signatures).toHaveLength(1);
  });

  it('blocks execution when signatures are insufficient or timelock is active', () => {
    service.propose('POOL_A', 'G_ADMIN_1');
    const check1 = service.canExecute('POOL_A');
    expect(check1.ready).toBe(false);
    expect(check1.reason).toContain('Insufficient signatures');

    service.signProposal('POOL_A', 'G_ADMIN_2');
    const check2 = service.canExecute('POOL_A');
    expect(check2.ready).toBe(false);
    expect(check2.reason).toContain('Timelock delay window');
  });
});
