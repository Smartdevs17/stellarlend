import React, { useCallback, useEffect, useState } from 'react';

interface ParameterProposal {
  id: number;
  pool: string;
  parameter: string;
  proposedValue: number;
  proposer: string;
  createdAt: number;
  effectiveAt: number;
  accepted: boolean;
  rejected: boolean;
  isEmergency: boolean;
}

interface VotingConfig {
  quorumBps: number;
  approvalThresholdBps: number;
  votingPeriodSeconds: number;
}

interface ParameterVote {
  proposalId: number;
  voter: string;
  support: boolean;
  weight: number;
  votedAt: number;
}

interface VoteTally {
  proposalId: number;
  forWeight: number;
  againstWeight: number;
  voterCount: number;
  totalVotingPower: number;
  participation: number;
  meetsQuorum: boolean;
  isApproved: boolean;
  hasPassed: boolean;
}

interface ProposalVotes {
  votes: ParameterVote[];
  tally: VoteTally;
}

interface GovernanceVotingProps {
  /** Connected wallet address. Voting is disabled without one. */
  voterAddress?: string;
}

const formatBps = (bps: number) => `${(bps / 100).toFixed(2)}%`;

/**
 * Voting interface for pending parameter-change proposals, backed by the
 * `/api/parameters` proposal and voting endpoints: shows the vote tally
 * against the quorum and approval threshold, and lets the connected wallet
 * vote for or against while the voting window is open.
 */
export const GovernanceVoting: React.FC<GovernanceVotingProps> = ({ voterAddress }) => {
  const [config, setConfig] = useState<VotingConfig | null>(null);
  const [proposals, setProposals] = useState<ParameterProposal[]>([]);
  const [votes, setVotes] = useState<Record<number, ProposalVotes>>({});
  const [loading, setLoading] = useState(true);
  const [votingOn, setVotingOn] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  const loadVotes = useCallback(async (id: number) => {
    const res = await fetch(`/api/parameters/proposals/${id}/votes`);
    const body = await res.json();
    if (body.success) {
      setVotes((prev) => ({ ...prev, [id]: body.data }));
    }
  }, []);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [configRes, proposalsRes] = await Promise.all([
        fetch('/api/parameters/voting/config'),
        fetch('/api/parameters/proposals?status=pending'),
      ]);
      const configBody = await configRes.json();
      const proposalsBody = await proposalsRes.json();
      if (!proposalsBody.success) {
        throw new Error(proposalsBody.error || 'Failed to load proposals');
      }
      setConfig(configBody.success ? configBody.data.config : null);
      setProposals(proposalsBody.data);
      await Promise.all(
        (proposalsBody.data as ParameterProposal[]).map((proposal) => loadVotes(proposal.id))
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load proposals');
    } finally {
      setLoading(false);
    }
  }, [loadVotes]);

  useEffect(() => {
    void load();
  }, [load]);

  // Keep the voting countdowns current.
  useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 30000);
    return () => clearInterval(timer);
  }, []);

  const castVote = async (id: number, support: boolean) => {
    if (!voterAddress) return;
    setVotingOn(id);
    setError(null);
    try {
      const res = await fetch(`/api/parameters/proposals/${id}/votes`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ voter: voterAddress, support }),
      });
      const body = await res.json();
      if (!body.success) {
        setError(body.error || 'Failed to cast vote');
        return;
      }
      await loadVotes(id);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to cast vote');
    } finally {
      setVotingOn(null);
    }
  };

  return (
    <div style={{ padding: 24, maxWidth: 900, margin: '0 auto', fontFamily: 'sans-serif' }}>
      <h2>Governance voting</h2>
      {config ? (
        <p>
          Quorum {formatBps(config.quorumBps)} of voting power · approval threshold{' '}
          {formatBps(config.approvalThresholdBps)} · voting period{' '}
          {(config.votingPeriodSeconds / 3600).toFixed(1)}h
        </p>
      ) : (
        !loading && <p>Voting is not enabled.</p>
      )}
      {!voterAddress && <p>Connect a wallet to vote.</p>}
      {error && <p style={{ color: '#c00' }}>{error}</p>}
      {loading && <p>Loading proposals…</p>}
      {!loading && proposals.length === 0 && <p>No proposals are pending.</p>}

      {proposals.map((proposal) => {
        const entry = votes[proposal.id];
        const tally = entry?.tally;
        const votingEndsAt = config ? proposal.createdAt + config.votingPeriodSeconds : null;
        const votingOpen = votingEndsAt !== null && now < votingEndsAt;
        const myVote = voterAddress
          ? entry?.votes.find((vote) => vote.voter === voterAddress)
          : undefined;
        const participationBps =
          tally && tally.totalVotingPower > 0
            ? (tally.participation * 10000) / tally.totalVotingPower
            : 0;
        const canVote = !!voterAddress && votingOpen && !myVote && votingOn === null;

        return (
          <div
            key={proposal.id}
            style={{ border: '1px solid #ddd', borderRadius: 8, padding: 16, marginBottom: 12 }}
          >
            <h3 style={{ margin: '0 0 8px' }}>
              #{proposal.id} · {proposal.parameter} → {proposal.proposedValue}
              {proposal.isEmergency && (
                <span style={{ color: '#c00', marginLeft: 8, fontSize: 14 }}>Emergency</span>
              )}
            </h3>
            <p style={{ margin: '4px 0' }}>
              Pool {proposal.pool} · proposed by {proposal.proposer}
            </p>
            <p style={{ margin: '4px 0' }}>
              {votingEndsAt === null
                ? 'Voting is not enabled'
                : votingOpen
                  ? `Voting ends ${new Date(votingEndsAt * 1000).toLocaleString()}`
                  : 'Voting closed'}
            </p>

            {tally && (
              <p style={{ margin: '4px 0' }}>
                For {tally.forWeight} · Against {tally.againstWeight} · {tally.voterCount} voter
                {tally.voterCount === 1 ? '' : 's'} · participation {formatBps(participationBps)} (
                {tally.meetsQuorum ? 'quorum reached' : 'quorum not reached'}) ·{' '}
                {tally.isApproved ? 'passing' : 'not passing'}
              </p>
            )}

            {myVote ? (
              <p style={{ margin: '8px 0 0' }}>
                You voted <strong>{myVote.support ? 'for' : 'against'}</strong> with weight{' '}
                {myVote.weight}.
              </p>
            ) : (
              <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
                <button onClick={() => castVote(proposal.id, true)} disabled={!canVote}>
                  Vote for
                </button>
                <button onClick={() => castVote(proposal.id, false)} disabled={!canVote}>
                  Vote against
                </button>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
};
