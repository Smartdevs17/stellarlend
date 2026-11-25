import { describe, it, expect, beforeEach } from 'vitest';
import { FlashloanSafelistManager } from '../flashloanReview';

describe('FlashloanSafelistManager', () => {
  let manager: FlashloanSafelistManager;

  beforeEach(() => {
    manager = new FlashloanSafelistManager();
  });

  it('approves registered contracts correctly', () => {
    const contract = 'CA3D52RZDSNTILPMCNDJA2AXJFFGUN7ISTDCH5I7DXKN23DNO83RG48K';
    manager.registerContract(contract, 'YieldOptimizer');
    expect(manager.isApproved(contract)).toBe(true);
  });

  it('rejects unregistered contracts', () => {
    expect(manager.isApproved('UNKNOWN_CONTRACT')).toBe(false);
  });

  it('handles revocation properly', () => {
    const contract = 'CA3D52RZDSNTILPMCNDJA2AXJFFGUN7ISTDCH5I7DXKN23DNO83RG48K';
    manager.registerContract(contract, 'ArbBot');
    expect(manager.isApproved(contract)).toBe(true);
    manager.revokeContract(contract);
    expect(manager.isApproved(contract)).toBe(false);
  });

  it('handles expiration correctly', () => {
    const past = new Date(Date.now() - 10000).toISOString();
    const contract = 'C_EXPIRED';
    manager.registerContract(contract, 'ExpiredCallback', past);
    expect(manager.isApproved(contract)).toBe(false);
  });
});
