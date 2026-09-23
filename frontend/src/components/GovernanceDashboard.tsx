import React, { useState, useEffect, useCallback } from 'react';

type ProposalStatus =
  | 'Pending'
  | 'Active'
  | 'Succeeded'
  | 'Defeated'
  | 'Expired'
  | 'Queued'
  | 'Executed'
  | 'Cancelled';

type ProposalKind =
  | 'MinCollateralRatio'
  | 'RiskParams'
  | 'PauseSwitch'
  | 'EmergencyPause'
  | 'GenericAction'
  | 'InterestRateConfig';

interface GovernanceProposal {
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

interface ProposalStats {
  total: number;
  byStatus: Record<ProposalStatus, number>;
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

const KINDS: ProposalKind[] = [
  'MinCollateralRatio',
  'RiskParams',
  'PauseSwitch',
  'EmergencyPause',
  'GenericAction',
  'InterestRateConfig',
];

const STATUS_COLORS: Record<ProposalStatus, string> = {
  Pending: '#94A3B8',
  Active: '#3B82F6',
  Succeeded: '#10B981',
  Defeated: '#EF4444',
  Expired: '#6B7280',
  Queued: '#F59E0B',
  Executed: '#059669',
  Cancelled: '#9CA3AF',
};

const TIMELINE: ProposalStatus[] = ['Pending', 'Active', 'Queued', 'Executed'];

export const GovernanceDashboard: React.FC = () => {
  const [proposals, setProposals] = useState<GovernanceProposal[]>([]);
  const [total, setTotal] = useState(0);
  const [stats, setStats] = useState<ProposalStats | null>(null);
  const [statusFilter, setStatusFilter] = useState('');
  const [kindFilter, setKindFilter] = useState('');
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState<GovernanceProposal | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  const loadProposals = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const params = new URLSearchParams({ limit: '50', offset: '0' });
      if (statusFilter) params.set('status', statusFilter);
      if (kindFilter) params.set('proposalType', kindFilter);
      if (search.trim()) params.set('search', search.trim());
      const res = await fetch(`/api/v1/governance/proposals?${params.toString()}`);
      if (!res.ok) throw new Error('Failed to load proposals');
      const json = await res.json();
      setProposals(json.data.proposals);
      setTotal(json.data.total);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [statusFilter, kindFilter, search]);

  const loadStats = useCallback(async () => {
    try {
      const res = await fetch('/api/v1/governance/proposals/stats/summary');
      if (!res.ok) return;
      const json = await res.json();
      setStats(json.data);
    } catch {
      // Stats are supplementary; the list remains usable without them.
    }
  }, []);

  useEffect(() => {
    loadProposals();
    loadStats();
  }, [loadProposals, loadStats]);

  const openDetail = async (id: string) => {
    try {
      const res = await fetch(`/api/v1/governance/proposals/${id}`);
      if (!res.ok) throw new Error('Failed to load proposal detail');
      const json = await res.json();
      setSelected(json.data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    }
  };

  const quorumPct = (p: GovernanceProposal): number => {
    if (p.totalVotingPower <= 0) return 0;
    return ((p.forVotes + p.againstVotes + p.abstainVotes) / p.totalVotingPower) * 100;
  };

  const voteShare = (p: GovernanceProposal): { forPct: number; againstPct: number } => {
    const cast = p.forVotes + p.againstVotes + p.abstainVotes;
    if (cast <= 0) return { forPct: 0, againstPct: 0 };
    return { forPct: (p.forVotes / cast) * 100, againstPct: (p.againstVotes / cast) * 100 };
  };

  return (
    <div style={styles.container}>
      <h2 style={styles.title}>Governance Dashboard</h2>
      <p style={styles.subtitle}>Proposal tracking across the full lifecycle: pending → active → queued → executed.</p>

      {stats && (
        <div style={styles.statsGrid}>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Total Proposals</div>
            <div style={styles.statValue}>{stats.total}</div>
          </div>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Active</div>
            <div style={{ ...styles.statValue, color: STATUS_COLORS.Active }}>{stats.byStatus.Active}</div>
          </div>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Queued</div>
            <div style={{ ...styles.statValue, color: STATUS_COLORS.Queued }}>{stats.byStatus.Queued}</div>
          </div>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Executed</div>
            <div style={{ ...styles.statValue, color: STATUS_COLORS.Executed }}>{stats.byStatus.Executed}</div>
          </div>
        </div>
      )}

      <div style={styles.controls}>
        <select value={statusFilter} onChange={(e) => setStatusFilter(e.target.value)} style={styles.select}>
          <option value="">All statuses</option>
          {STATUSES.map((s) => (
            <option key={s} value={s}>{s}</option>
          ))}
        </select>
        <select value={kindFilter} onChange={(e) => setKindFilter(e.target.value)} style={styles.select}>
          <option value="">All types</option>
          {KINDS.map((k) => (
            <option key={k} value={k}>{k}</option>
          ))}
        </select>
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search id, description, proposer"
          style={styles.search}
        />
      </div>

      {loading && <p>Loading proposals…</p>}
      {error && <p style={{ color: '#d32f2f' }}>{error}</p>}

      {!loading && !error && (
        <p style={styles.subtitle}>{total} proposal{total === 1 ? '' : 's'} found</p>
      )}

      <div style={styles.tableWrap}>
        <table style={styles.table}>
          <thead>
            <tr>
              <th style={styles.th}>ID</th>
              <th style={styles.th}>Description</th>
              <th style={styles.th}>Type</th>
              <th style={styles.th}>Status</th>
              <th style={styles.th}>For / Against</th>
              <th style={styles.th}>Quorum</th>
            </tr>
          </thead>
          <tbody>
            {proposals.map((p) => {
              const { forPct, againstPct } = voteShare(p);
              return (
                <tr key={p.id} onClick={() => openDetail(p.id)} style={styles.row}>
                  <td style={styles.td}>#{p.id}</td>
                  <td style={styles.td}>{p.description}</td>
                  <td style={styles.td}>{p.proposalType}</td>
                  <td style={styles.td}>
                    <span style={{ ...styles.badge, backgroundColor: STATUS_COLORS[p.status] }}>
                      {p.status}
                    </span>
                  </td>
                  <td style={styles.td}>
                    <div style={styles.bar}>
                      <div style={{ ...styles.barFor, width: `${forPct}%` }} />
                      <div style={{ ...styles.barAgainst, width: `${againstPct}%` }} />
                    </div>
                    <small>{p.forVotes.toLocaleString()} / {p.againstVotes.toLocaleString()}</small>
                  </td>
                  <td style={styles.td}>{quorumPct(p).toFixed(1)}%</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {selected && (
        <div style={styles.detail}>
          <h3>Proposal #{selected.id} — {selected.status}</h3>
          <p>{selected.description}</p>
          <div style={styles.detailGrid}>
            <div><strong>Type:</strong> {selected.proposalType}</div>
            <div><strong>Proposer:</strong> {selected.proposer}</div>
            <div><strong>Voting window:</strong> {new Date(selected.startTime).toLocaleString()} → {new Date(selected.endTime).toLocaleString()}</div>
            <div><strong>Execution:</strong> {selected.executionTime ? new Date(selected.executionTime).toLocaleString() : '—'}</div>
            <div><strong>Threshold:</strong> {(selected.votingThresholdBps / 100).toFixed(0)}%</div>
            <div>
              <strong>Votes:</strong> {selected.forVotes.toLocaleString()} for / {selected.againstVotes.toLocaleString()} against / {selected.abstainVotes.toLocaleString()} abstain
            </div>
          </div>
          <h4>Lifecycle</h4>
          <div style={styles.timeline}>
            {TIMELINE.map((step) => {
              const reached =
                TIMELINE.indexOf(step) <= TIMELINE.indexOf(selected.status as ProposalStatus) ||
                ['Succeeded', 'Executed'].includes(selected.status);
              const failed = ['Defeated', 'Expired', 'Cancelled'].includes(selected.status) && step === 'Queued';
              return (
                <span
                  key={step}
                  style={{
                    ...styles.timelineStep,
                    backgroundColor: failed ? STATUS_COLORS.Defeated : reached ? STATUS_COLORS.Active : '#E5E7EB',
                    color: reached || failed ? '#fff' : '#374151',
                  }}
                >
                  {step}
                </span>
              );
            })}
            {['Defeated', 'Expired', 'Cancelled'].includes(selected.status) && (
              <span style={{ ...styles.timelineStep, backgroundColor: STATUS_COLORS[selected.status], color: '#fff' }}>
                {selected.status}
              </span>
            )}
          </div>
          <p style={styles.subtitle}>
            Dry-run this proposal in the Governance Proposal Simulator with proposal ID #{selected.id}.
          </p>
          <button onClick={() => setSelected(null)} style={styles.button}>Close</button>
        </div>
      )}
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: { padding: 24, maxWidth: 1100, margin: '0 auto', fontFamily: 'sans-serif' },
  title: { margin: '0 0 4px' },
  subtitle: { color: '#64748B', margin: '0 0 16px' },
  statsGrid: { display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12, marginBottom: 16 },
  statCard: { border: '1px solid #E5E7EB', borderRadius: 8, padding: 12 },
  statLabel: { fontSize: 12, color: '#64748B' },
  statValue: { fontSize: 24, fontWeight: 700 },
  controls: { display: 'flex', gap: 8, marginBottom: 16 },
  select: { padding: 8, borderRadius: 6, border: '1px solid #CBD5E1' },
  search: { flex: 1, padding: 8, borderRadius: 6, border: '1px solid #CBD5E1' },
  tableWrap: { overflowX: 'auto' },
  table: { width: '100%', borderCollapse: 'collapse' },
  th: { textAlign: 'left', fontSize: 12, color: '#64748B', borderBottom: '2px solid #E5E7EB', padding: 8 },
  td: { borderBottom: '1px solid #F1F5F9', padding: 8, fontSize: 14 },
  row: { cursor: 'pointer' },
  badge: { color: '#fff', fontSize: 12, padding: '2px 8px', borderRadius: 12 },
  bar: { display: 'flex', height: 8, borderRadius: 4, overflow: 'hidden', backgroundColor: '#F1F5F9', marginBottom: 4 },
  barFor: { backgroundColor: '#10B981' },
  barAgainst: { backgroundColor: '#EF4444' },
  detail: { marginTop: 16, border: '1px solid #E5E7EB', borderRadius: 8, padding: 16 },
  detailGrid: { display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8, fontSize: 14, marginBottom: 8 },
  timeline: { display: 'flex', gap: 8, marginBottom: 8 },
  timelineStep: { fontSize: 12, padding: '4px 12px', borderRadius: 12 },
  button: { padding: '8px 16px', borderRadius: 6, border: '1px solid #CBD5E1', cursor: 'pointer' },
};
