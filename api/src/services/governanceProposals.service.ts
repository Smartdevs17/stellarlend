import logger from '../utils/logger';

/**
 * Proposal tracking for the governance dashboard (Issue #1087).
 *
 * Shapes mirror the on-chain governance contract
 * (`stellar-lend/contracts/hello-world/src/types.rs`):
 * status lifecycle Pending → Active → Queued → Executed, with
 * Succeeded / Defeated / Expired / Cancelled as terminal states.
 */

export type ProposalStatus =
  | 'Pending'
  | 'Active'
  | 'Succeeded'
  | 'Defeated'
  | 'Expired'
  | 'Queued'
  | 'Executed'
  | 'Cancelled';

export type ProposalKind =
  | 'MinCollateralRatio'
  | 'RiskParams'
  | 'PauseSwitch'
  | 'EmergencyPause'
  | 'GenericAction'
  | 'InterestRateConfig';

export interface GovernanceProposal {
  id: string;
  proposer: string;
  proposalType: ProposalKind;
  description: string;
  status: ProposalStatus;
  startTime: string;
  endTime: string;
  executionTime: string | null;
  votingThresholdBps: number;
  forVotes: number;
  againstVotes: number;
  abstainVotes: number;
  totalVotingPower: number;
  createdAt: string;
}

export interface ProposalFilters {
  status?: string;
  proposalType?: string;
  search?: string;
  limit?: number;
  offset?: number;
}

export interface ProposalListResult {
  proposals: GovernanceProposal[];
  total: number;
  limit: number;
  offset: number;
}

export interface ProposalStats {
  total: number;
  byStatus: Record<ProposalStatus, number>;
}

const DAY_MS = 86_400_000;
const iso = (ms: number): string => new Date(ms).toISOString();

function seedProposals(): GovernanceProposal[] {
  const now = Date.now();
  return [
    {
      id: '1',
      proposer: 'GOVERNANCE_ADMIN',
      proposalType: 'EmergencyPause',
      description: 'Emergency pause of borrow operations during oracle outage',
      status: 'Executed',
      startTime: iso(now - 9 * DAY_MS),
      endTime: iso(now - 7 * DAY_MS),
      executionTime: iso(now - 6 * DAY_MS),
      votingThresholdBps: 5000,
      forVotes: 820_000,
      againstVotes: 40_000,
      abstainVotes: 10_000,
      totalVotingPower: 1_000_000,
      createdAt: iso(now - 10 * DAY_MS),
    },
    {
      id: '2',
      proposer: 'GOVERNANCE_ADMIN',
      proposalType: 'MinCollateralRatio',
      description: 'Raise minimum collateral ratio from 150% to 160%',
      status: 'Queued',
      startTime: iso(now - 3 * DAY_MS),
      endTime: iso(now - 1 * DAY_MS),
      executionTime: iso(now + 1 * DAY_MS),
      votingThresholdBps: 5000,
      forVotes: 610_000,
      againstVotes: 180_000,
      abstainVotes: 25_000,
      totalVotingPower: 1_000_000,
      createdAt: iso(now - 4 * DAY_MS),
    },
    {
      id: '3',
      proposer: 'RISK_COUNCIL',
      proposalType: 'RiskParams',
      description: 'Tighten liquidation threshold and close factor for volatile assets',
      status: 'Active',
      startTime: iso(now - 1 * DAY_MS),
      endTime: iso(now + 2 * DAY_MS),
      executionTime: null,
      votingThresholdBps: 5000,
      forVotes: 320_000,
      againstVotes: 90_000,
      abstainVotes: 15_000,
      totalVotingPower: 1_000_000,
      createdAt: iso(now - 2 * DAY_MS),
    },
    {
      id: '4',
      proposer: 'RISK_COUNCIL',
      proposalType: 'InterestRateConfig',
      description: 'Lower stable-rate premium after utilization normalizes',
      status: 'Pending',
      startTime: iso(now + 1 * DAY_MS),
      endTime: iso(now + 4 * DAY_MS),
      executionTime: null,
      votingThresholdBps: 5000,
      forVotes: 0,
      againstVotes: 0,
      abstainVotes: 0,
      totalVotingPower: 1_000_000,
      createdAt: iso(now - 6 * 3_600_000),
    },
    {
      id: '5',
      proposer: 'GOVERNANCE_ADMIN',
      proposalType: 'PauseSwitch',
      description: 'Unpause flash loans after security review sign-off',
      status: 'Succeeded',
      startTime: iso(now - 6 * DAY_MS),
      endTime: iso(now - 4 * DAY_MS),
      executionTime: null,
      votingThresholdBps: 5000,
      forVotes: 540_000,
      againstVotes: 120_000,
      abstainVotes: 30_000,
      totalVotingPower: 1_000_000,
      createdAt: iso(now - 7 * DAY_MS),
    },
    {
      id: '6',
      proposer: 'COMMUNITY_MULTISIG',
      proposalType: 'GenericAction',
      description: 'Treasury grant for oracle redundancy pilot',
      status: 'Defeated',
      startTime: iso(now - 12 * DAY_MS),
      endTime: iso(now - 10 * DAY_MS),
      executionTime: null,
      votingThresholdBps: 5000,
      forVotes: 210_000,
      againstVotes: 480_000,
      abstainVotes: 20_000,
      totalVotingPower: 1_000_000,
      createdAt: iso(now - 13 * DAY_MS),
    },
    {
      id: '7',
      proposer: 'COMMUNITY_MULTISIG',
      proposalType: 'RiskParams',
      description: 'Withdrawn: duplicate of proposal #3 with stale thresholds',
      status: 'Cancelled',
      startTime: iso(now - 5 * DAY_MS),
      endTime: iso(now - 2 * DAY_MS),
      executionTime: null,
      votingThresholdBps: 5000,
      forVotes: 12_000,
      againstVotes: 8_000,
      abstainVotes: 1_000,
      totalVotingPower: 1_000_000,
      createdAt: iso(now - 6 * DAY_MS),
    },
  ];
}

const STATUSES: ProposalStatus[] = [
  'Pending',
  'Active',
  'Succeeded',
  'Defeated',
  'Expired',
  'Queued',
  'Executed',
  'Cancelled',
];

class GovernanceProposalsService {
  private readonly proposals: GovernanceProposal[] = seedProposals();

  listProposals(filters: ProposalFilters = {}): ProposalListResult {
    const limit = Math.min(Math.max(filters.limit ?? 20, 1), 100);
    const offset = Math.max(filters.offset ?? 0, 0);
    const search = (filters.search ?? '').trim().toLowerCase();

    const filtered = this.proposals.filter((p) => {
      if (filters.status && p.status !== filters.status) return false;
      if (filters.proposalType && p.proposalType !== filters.proposalType) return false;
      if (search && !`${p.id} ${p.description} ${p.proposer}`.toLowerCase().includes(search)) {
        return false;
      }
      return true;
    });

    logger.info('Governance proposals listed', {
      total: filtered.length,
      status: filters.status,
      proposalType: filters.proposalType,
    });

    return {
      proposals: filtered.slice(offset, offset + limit),
      total: filtered.length,
      limit,
      offset,
    };
  }

  getProposal(id: string): GovernanceProposal | undefined {
    return this.proposals.find((p) => p.id === id);
  }

  getStats(): ProposalStats {
    const byStatus = Object.fromEntries(STATUSES.map((s) => [s, 0])) as Record<
      ProposalStatus,
      number
    >;
    for (const p of this.proposals) {
      byStatus[p.status] += 1;
    }
    return { total: this.proposals.length, byStatus };
  }
}

export const governanceProposalsService = new GovernanceProposalsService();
