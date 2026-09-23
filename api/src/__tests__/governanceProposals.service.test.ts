import { governanceProposalsService } from '../services/governanceProposals.service';

describe('GovernanceProposalsService (Issue #1087)', () => {
  it('lists seeded proposals with pagination metadata', () => {
    const result = governanceProposalsService.listProposals({});
    expect(result.total).toBeGreaterThan(0);
    expect(result.proposals.length).toBeLessThanOrEqual(result.limit);
    expect(result).toMatchObject({ limit: 20, offset: 0 });
  });

  it('filters by status', () => {
    const result = governanceProposalsService.listProposals({ status: 'Active' });
    expect(result.total).toBeGreaterThanOrEqual(1);
    for (const p of result.proposals) {
      expect(p.status).toBe('Active');
    }
  });

  it('searches by description text', () => {
    const result = governanceProposalsService.listProposals({ search: 'oracle' });
    expect(result.total).toBeGreaterThanOrEqual(1);
    expect(result.proposals[0].description.toLowerCase()).toContain('oracle');
  });

  it('returns proposal detail by id and 404-shape for unknown ids', () => {
    const proposal = governanceProposalsService.getProposal('1');
    expect(proposal).toBeDefined();
    expect(proposal?.id).toBe('1');
    expect(governanceProposalsService.getProposal('no-such-id')).toBeUndefined();
  });

  it('reports counts by status', () => {
    const stats = governanceProposalsService.getStats();
    expect(stats.total).toBeGreaterThan(0);
    const summed = Object.values(stats.byStatus).reduce((a, b) => a + b, 0);
    expect(summed).toBe(stats.total);
  });
});
