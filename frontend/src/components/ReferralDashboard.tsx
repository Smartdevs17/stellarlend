import React, { useEffect, useState } from 'react';

interface ReferralStats {
  code: string;
  totalReferrals: number;
  l2Referrals: number;
  totalEarned: number;
  totalClaimed: number;
  claimable: number;
  tier: number;
}

interface ConversionFunnel {
  referralCode: string;
  referralsGenerated: number;
  referralsConverted: number;
  conversionRate: string;
  l2Referrals: number;
}

interface LeaderboardEntry {
  rank: number;
  userAddress: string;
  maskedAddress: string;
  code: string;
  totalReferrals: number;
  l2Referrals: number;
  totalEarned: number;
  claimable: number;
  tier: number;
  tierLabel: string;
}

interface GlobalAnalytics {
  totalAffiliates: number;
  totalReferees: number;
  totalFeesGenerated: number;
  totalRewardsDistributed: number;
  claimableBalanceProtocol: number;
  conversionRate: string;
  averageEarnedPerAffiliate: number;
}

interface ProgramConfig {
  l1FeeSharePct: number;
  l2FeeSharePct: number;
  maturityDays: number;
  minDepositQualify: number;
}

export const ReferralDashboard: React.FC = () => {
  const [stats, setStats] = useState<ReferralStats | null>(null);
  const [funnel, setFunnel] = useState<ConversionFunnel | null>(null);
  const [leaderboard, setLeaderboard] = useState<LeaderboardEntry[]>([]);
  const [analytics, setAnalytics] = useState<GlobalAnalytics | null>(null);
  const [config, setConfig] = useState<ProgramConfig | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [referralLink, setReferralLink] = useState<string>('');
  const [activeTab, setActiveTab] = useState<'overview' | 'leaderboard' | 'analytics' | 'config'>('overview');

  useEffect(() => {
    const fetchAllData = async () => {
      try {
        const statsRes = await fetch(`/api/referral/stats?userAddress=current`);
        const statsData = await statsRes.json();
        if (statsData.success) {
          setStats(statsData.data);
          setReferralLink(statsData.data.code ? `https://stellarlend.com?ref=${statsData.data.code}` : '');
        }

        const funnelRes = await fetch(`/api/referral/funnel?userAddress=current`);
        const funnelData = await funnelRes.json();
        if (funnelData.success) {
          setFunnel(funnelData.data);
        }

        const lbRes = await fetch(`/api/referral/leaderboard?limit=10`);
        const lbData = await lbRes.json();
        if (lbData.success) {
          setLeaderboard(lbData.data);
        }

        const analyticsRes = await fetch(`/api/referral/analytics`);
        const analyticsData = await analyticsRes.json();
        if (analyticsData.success) {
          setAnalytics(analyticsData.data);
        }

        const configRes = await fetch(`/api/referral/config`);
        const configData = await configRes.json();
        if (configData.success) {
          setConfig(configData.data);
        }

        setIsLoading(false);
      } catch (error) {
        console.error('Failed to fetch referral data:', error);
        setIsLoading(false);
      }
    };

    fetchAllData();
  }, []);

  const getTierBadge = (tier: number) => {
    switch (tier) {
      case 3:
        return { label: 'Tier 3 - Diamond', color: '#b9f2ff' };
      case 2:
        return { label: 'Tier 2 - Gold', color: '#FFD700' };
      case 1:
        return { label: 'Tier 1 - Silver', color: '#C0C0C0' };
      default:
        return { label: 'Tier 0 - Bronze', color: '#CD7F32' };
    }
  };

  const handleClaim = async () => {
    if (!stats || stats.claimable <= 0) return;

    try {
      const response = await fetch('/api/referral/claim', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userAddress: 'current' }),
      });
      if (response.ok) {
        const newResponse = await fetch(`/api/referral/stats?userAddress=current`);
        const newData = await newResponse.json();
        if (newData.success) setStats(newData.data);
      }
    } catch (error) {
      console.error('Claim failed:', error);
    }
  };

  const handleGenerateCode = async () => {
    try {
      const response = await fetch('/api/referral/generate-code', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userAddress: 'current' }),
      });
      const data = await response.json();
      if (data.success) {
        setReferralLink(data.data.link);
        setStats((prev) => (prev ? { ...prev, code: data.data.code } : null));
      }
    } catch (error) {
      console.error('Failed to generate code:', error);
    }
  };

  const copyToClipboard = () => {
    navigator.clipboard.writeText(referralLink);
    alert('Referral link copied to clipboard!');
  };

  if (isLoading) {
    return <div>Loading referral dashboard...</div>;
  }

  const tier = stats ? getTierBadge(stats.tier) : null;

  return (
    <div style={styles.container}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
        <h2>Referral & Affiliate Program</h2>
        <div style={{ display: 'flex', gap: '8px' }}>
          {(['overview', 'leaderboard', 'analytics', 'config'] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              style={{
                padding: '6px 14px',
                borderRadius: '6px',
                border: '1px solid #ccc',
                backgroundColor: activeTab === tab ? '#007bff' : 'white',
                color: activeTab === tab ? 'white' : '#333',
                cursor: 'pointer',
                fontWeight: 'bold',
                fontSize: '13px',
                textTransform: 'capitalize',
              }}
            >
              {tab}
            </button>
          ))}
        </div>
      </div>

      {activeTab === 'overview' && stats && (
        <>
          <div style={styles.headerSection}>
            <div style={{ ...styles.tierBadge, backgroundColor: tier?.color }}>
              {tier?.label}
            </div>
          </div>

          <div style={styles.metricsGrid}>
            <div style={styles.metric}>
              <span style={styles.label}>Direct Referrals (L1)</span>
              <span style={styles.value}>{stats.totalReferrals}</span>
            </div>
            <div style={styles.metric}>
              <span style={styles.label}>Network Referrals (L2)</span>
              <span style={styles.value}>{stats.l2Referrals}</span>
            </div>
            <div style={styles.metric}>
              <span style={styles.label}>Total Earned</span>
              <span style={styles.value}>${stats.totalEarned.toFixed(2)}</span>
            </div>
            <div style={styles.metric}>
              <span style={styles.label}>Claimable Rewards</span>
              <span style={styles.value}>${stats.claimable.toFixed(2)}</span>
            </div>
          </div>

          <div style={styles.section}>
            <h3>Your Unique Referral Link</h3>
            <div style={styles.referralLink}>
              <input
                type="text"
                value={referralLink || 'No code generated yet'}
                readOnly
                style={styles.linkInput}
              />
              <button onClick={copyToClipboard} style={styles.copyButton}>
                Copy Link
              </button>
              {!referralLink && (
                <button onClick={handleGenerateCode} style={{ ...styles.copyButton, backgroundColor: '#17a2b8' }}>
                  Generate Code
                </button>
              )}
            </div>
          </div>

          {funnel && (
            <div style={styles.section}>
              <h3>Conversion Funnel</h3>
              <div style={styles.funnelMetrics}>
                <p>Referral Clicks / Generated: {funnel.referralsGenerated}</p>
                <p>Converted Lenders: {funnel.referralsConverted}</p>
                <p>Conversion Rate: {funnel.conversionRate}%</p>
              </div>
            </div>
          )}

          <div style={styles.section}>
            <h3>Affiliate Reward Distribution</h3>
            <div style={styles.earningsSection}>
              <p>Total Claimed: ${stats.totalClaimed.toFixed(2)}</p>
              <p>Available to Claim: ${stats.claimable.toFixed(2)}</p>
              <button
                onClick={handleClaim}
                disabled={stats.claimable <= 0}
                style={{
                  ...styles.claimButton,
                  opacity: stats.claimable > 0 ? 1 : 0.5,
                  cursor: stats.claimable > 0 ? 'pointer' : 'not-allowed',
                }}
              >
                Claim Rewards (Instant Payout)
              </button>
            </div>
          </div>
        </>
      )}

      {activeTab === 'leaderboard' && (
        <div style={styles.section}>
          <h3>Affiliate Leaderboard</h3>
          <p style={{ fontSize: '13px', color: '#666', marginBottom: '16px' }}>
            Top lending protocol affiliates ranked by cumulative earnings and volume.
          </p>
          <table style={{ width: '100%', borderCollapse: 'collapse', textAlign: 'left' }}>
            <thead>
              <tr style={{ borderBottom: '2px solid #ddd', fontSize: '13px', color: '#666' }}>
                <th style={{ padding: '8px' }}>Rank</th>
                <th style={{ padding: '8px' }}>Affiliate</th>
                <th style={{ padding: '8px' }}>Referrals</th>
                <th style={{ padding: '8px' }}>Tier</th>
                <th style={{ padding: '8px' }}>Total Earned</th>
              </tr>
            </thead>
            <tbody>
              {leaderboard.length === 0 ? (
                <tr>
                  <td colSpan={5} style={{ padding: '16px', textAlign: 'center', color: '#999' }}>
                    No leaderboard data yet. Start referring to lead!
                  </td>
                </tr>
              ) : (
                leaderboard.map((entry) => (
                  <tr key={entry.rank} style={{ borderBottom: '1px solid #eee' }}>
                    <td style={{ padding: '10px 8px', fontWeight: 'bold' }}>#{entry.rank}</td>
                    <td style={{ padding: '10px 8px' }}>{entry.maskedAddress} ({entry.code})</td>
                    <td style={{ padding: '10px 8px' }}>{entry.totalReferrals}</td>
                    <td style={{ padding: '10px 8px' }}>
                      <span style={{ padding: '2px 8px', borderRadius: '10px', fontSize: '11px', backgroundColor: '#e2e8f0' }}>
                        {entry.tierLabel}
                      </span>
                    </td>
                    <td style={{ padding: '10px 8px', fontWeight: 'bold', color: '#28a745' }}>
                      ${entry.totalEarned.toFixed(2)}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      )}

      {activeTab === 'analytics' && analytics && (
        <div style={styles.section}>
          <h3>Global Protocol Referral Analytics</h3>
          <div style={styles.metricsGrid}>
            <div style={styles.metric}>
              <span style={styles.label}>Total Active Affiliates</span>
              <span style={styles.value}>{analytics.totalAffiliates}</span>
            </div>
            <div style={styles.metric}>
              <span style={styles.label}>Total Referees Onboarded</span>
              <span style={styles.value}>{analytics.totalReferees}</span>
            </div>
            <div style={styles.metric}>
              <span style={styles.label}>Protocol Referral Fees</span>
              <span style={styles.value}>${analytics.totalFeesGenerated.toFixed(2)}</span>
            </div>
            <div style={styles.metric}>
              <span style={styles.label}>Affiliate Rewards Distributed</span>
              <span style={styles.value}>${analytics.totalRewardsDistributed.toFixed(2)}</span>
            </div>
          </div>
        </div>
      )}

      {activeTab === 'config' && config && (
        <div style={styles.section}>
          <h3>Referral Program Configuration</h3>
          <p style={{ fontSize: '13px', color: '#666', marginBottom: '16px' }}>
            Current parameters governed by protocol smart contracts and backend settings.
          </p>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px', fontSize: '14px' }}>
            <div style={{ padding: '10px', backgroundColor: '#fff', border: '1px solid #ddd', borderRadius: '6px' }}>
              <strong>Direct Fee Share (L1):</strong> {config.l1FeeSharePct}%
            </div>
            <div style={{ padding: '10px', backgroundColor: '#fff', border: '1px solid #ddd', borderRadius: '6px' }}>
              <strong>Secondary Fee Share (L2):</strong> {config.l2FeeSharePct}%
            </div>
            <div style={{ padding: '10px', backgroundColor: '#fff', border: '1px solid #ddd', borderRadius: '6px' }}>
              <strong>Reward Maturity Period:</strong> {config.maturityDays} Days
            </div>
            <div style={{ padding: '10px', backgroundColor: '#fff', border: '1px solid #ddd', borderRadius: '6px' }}>
              <strong>Anti-Sybil Min Deposit:</strong> 100 Tokens
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    padding: '20px',
    maxWidth: '850px',
    margin: '0 auto',
  },
  headerSection: {
    marginBottom: '20px',
    textAlign: 'center',
  },
  tierBadge: {
    display: 'inline-block',
    padding: '8px 16px',
    color: 'black',
    borderRadius: '20px',
    fontWeight: 'bold',
  },
  metricsGrid: {
    display: 'grid',
    gridTemplateColumns: 'repeat(2, 1fr)',
    gap: '15px',
    marginBottom: '30px',
  },
  metric: {
    padding: '15px',
    backgroundColor: '#f9f9f9',
    borderRadius: '8px',
    textAlign: 'center',
    border: '1px solid #e0e0e0',
  },
  label: {
    display: 'block',
    fontSize: '12px',
    color: '#666',
    marginBottom: '5px',
  },
  value: {
    display: 'block',
    fontSize: '24px',
    fontWeight: 'bold',
    color: '#333',
  },
  section: {
    marginBottom: '30px',
    padding: '20px',
    backgroundColor: '#f9f9f9',
    borderRadius: '8px',
    border: '1px solid #e0e0e0',
  },
  referralLink: {
    display: 'flex',
    gap: '10px',
    marginTop: '10px',
  },
  linkInput: {
    flex: 1,
    padding: '8px 12px',
    borderRadius: '4px',
    border: '1px solid #ddd',
    fontFamily: 'monospace',
    fontSize: '12px',
  },
  copyButton: {
    padding: '8px 16px',
    backgroundColor: '#007bff',
    color: 'white',
    border: 'none',
    borderRadius: '4px',
    cursor: 'pointer',
  },
  funnelMetrics: {
    marginTop: '10px',
  },
  earningsSection: {
    marginTop: '10px',
  },
  claimButton: {
    marginTop: '15px',
    padding: '10px 20px',
    backgroundColor: '#28a745',
    color: 'white',
    border: 'none',
    borderRadius: '4px',
    cursor: 'pointer',
  },
};
