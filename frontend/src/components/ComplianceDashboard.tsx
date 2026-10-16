import React, { useState, useEffect, useCallback } from 'react';

interface ComplianceSummary {
  totalKycVerified: number;
  totalSanctioned: number;
  totalSars: number;
  totalAmlAlerts: number;
  pendingReviews: number;
  complianceRate: number;
}

interface ComplianceEvent {
  id: string;
  eventType: string;
  address: string;
  details?: string;
  timestamp: string;
}

interface DashboardData {
  summary: ComplianceSummary;
  recentEvents: ComplianceEvent[];
  riskDistribution: { low: number; medium: number; high: number; critical: number };
  jurisdictionStats: Record<string, { count: number; riskScore: number }>;
  topFlaggedAddresses: Array<{ address: string; flagCount: number; lastFlag: string }>;
  regulatoryLimits: {
    dailyVolumeUsed: string;
    dailyVolumeLimit: string;
    weeklyVolumeUsed: string;
    weeklyVolumeLimit: string;
  };
}

export const ComplianceDashboard: React.FC = () => {
  const [data, setData] = useState<DashboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  const loadDashboard = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const res = await fetch('/api/compliance/dashboard');
      if (!res.ok) throw new Error('Failed to load compliance dashboard');
      const json = await res.json();
      setData(json);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadDashboard();
  }, [loadDashboard]);

  const formatVolume = (stroops: string): string => {
    const xlm = Number(stroops) / 1e7;
    return xlm >= 1000 ? `${(xlm / 1000).toFixed(1)}K XLM` : `${xlm.toFixed(0)} XLM`;
  };

  if (loading) return <div style={styles.container}><p>Loading compliance dashboard…</p></div>;
  if (error) return <div style={styles.container}><p style={{ color: '#d32f2f' }}>{error}</p></div>;
  if (!data) return null;

  const { summary, recentEvents, riskDistribution, jurisdictionStats, topFlaggedAddresses, regulatoryLimits } = data;

  return (
    <div style={styles.container}>
      <h2 style={styles.title}>Compliance Dashboard</h2>

      <div style={styles.statsGrid}>
        <div style={styles.statCard}>
          <div style={styles.statLabel}>KYC Verified</div>
          <div style={styles.statValue}>{summary.totalKycVerified}</div>
        </div>
        <div style={styles.statCard}>
          <div style={styles.statLabel}>Compliance Rate</div>
          <div style={{ ...styles.statValue, color: summary.complianceRate >= 95 ? '#2e7d32' : '#e65100' }}>
            {summary.complianceRate.toFixed(1)}%
          </div>
        </div>
        <div style={styles.statCard}>
          <div style={styles.statLabel}>SARs Filed</div>
          <div style={styles.statValue}>{summary.totalSars}</div>
        </div>
        <div style={styles.statCard}>
          <div style={styles.statLabel}>Pending Reviews</div>
          <div style={{ ...styles.statValue, color: summary.pendingReviews > 0 ? '#d32f2f' : '#2e7d32' }}>
            {summary.pendingReviews}
          </div>
        </div>
        <div style={styles.statCard}>
          <div style={styles.statLabel}>Sanctioned</div>
          <div style={styles.statValue}>{summary.totalSanctioned}</div>
        </div>
        <div style={styles.statCard}>
          <div style={styles.statLabel}>AML Alerts</div>
          <div style={styles.statValue}>{summary.totalAmlAlerts}</div>
        </div>
      </div>

      <div style={styles.row}>
        <div style={styles.card}>
          <h3 style={styles.sectionTitle}>AML Risk Distribution</h3>
          <div style={styles.riskGrid}>
            {(['low', 'medium', 'high', 'critical'] as const).map((level) => (
              <div key={level} style={styles.riskItem}>
                <span style={{ ...styles.riskBadge, backgroundColor: RISK_COLORS[level] }}>{level}</span>
                <span style={styles.riskCount}>{riskDistribution[level]}</span>
              </div>
            ))}
          </div>
        </div>

        <div style={styles.card}>
          <h3 style={styles.sectionTitle}>Regulatory Limits</h3>
          <div style={styles.limitRow}>
            <span>Daily Volume</span>
            <span>{formatVolume(regulatoryLimits.dailyVolumeUsed)} / {formatVolume(regulatoryLimits.dailyVolumeLimit)}</span>
          </div>
          <div style={styles.limitRow}>
            <span>Weekly Volume</span>
            <span>{formatVolume(regulatoryLimits.weeklyVolumeUsed)} / {formatVolume(regulatoryLimits.weeklyVolumeLimit)}</span>
          </div>
        </div>
      </div>

      {Object.keys(jurisdictionStats).length > 0 && (
        <div style={styles.card}>
          <h3 style={styles.sectionTitle}>Jurisdiction Breakdown</h3>
          <table style={styles.table}>
            <thead>
              <tr>
                <th style={styles.th}>Jurisdiction</th>
                <th style={styles.th}>Users</th>
                <th style={styles.th}>Max Risk Score</th>
              </tr>
            </thead>
            <tbody>
              {Object.entries(jurisdictionStats).map(([jurisdiction, stats]) => (
                <tr key={jurisdiction}>
                  <td style={styles.td}>{jurisdiction}</td>
                  <td style={styles.td}>{stats.count}</td>
                  <td style={styles.td}>{stats.riskScore}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {topFlaggedAddresses.length > 0 && (
        <div style={styles.card}>
          <h3 style={styles.sectionTitle}>Top Flagged Addresses</h3>
          <table style={styles.table}>
            <thead>
              <tr>
                <th style={styles.th}>Address</th>
                <th style={styles.th}>Flags</th>
                <th style={styles.th}>Last Flag</th>
              </tr>
            </thead>
            <tbody>
              {topFlaggedAddresses.map((entry) => (
                <tr key={entry.address}>
                  <td style={styles.td}>{entry.address.slice(0, 12)}…</td>
                  <td style={styles.td}>{entry.flagCount}</td>
                  <td style={styles.td}>{new Date(entry.lastFlag).toLocaleDateString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div style={styles.card}>
        <h3 style={styles.sectionTitle}>Recent Audit Events</h3>
        {recentEvents.length === 0 ? (
          <p style={styles.empty}>No compliance events recorded yet.</p>
        ) : (
          <table style={styles.table}>
            <thead>
              <tr>
                <th style={styles.th}>Type</th>
                <th style={styles.th}>Address</th>
                <th style={styles.th}>Details</th>
                <th style={styles.th}>Time</th>
              </tr>
            </thead>
            <tbody>
              {recentEvents.slice().reverse().map((event) => (
                <tr key={event.id}>
                  <td style={styles.td}>{event.eventType}</td>
                  <td style={styles.td}>{event.address.slice(0, 12)}…</td>
                  <td style={styles.td}>{event.details ?? '—'}</td>
                  <td style={styles.td}>{new Date(event.timestamp).toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
};

const RISK_COLORS = { low: '#2e7d32', medium: '#f9a825', high: '#e65100', critical: '#d32f2f' };

const styles: Record<string, React.CSSProperties> = {
  container: { padding: '24px', maxWidth: '1100px', margin: '0 auto', fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif' },
  title: { fontSize: '24px', fontWeight: 600, marginBottom: '24px', color: '#1a1a2e' },
  statsGrid: { display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '12px', marginBottom: '16px' },
  statCard: { padding: '16px', backgroundColor: '#fff', borderRadius: '8px', border: '1px solid #e0e0e0', textAlign: 'center' },
  statLabel: { fontSize: '11px', color: '#666', textTransform: 'uppercase', fontWeight: 600, marginBottom: '4px' },
  statValue: { fontSize: '22px', fontWeight: 700, color: '#1a1a2e' },
  row: { display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '16px', marginBottom: '16px' },
  card: { backgroundColor: '#fff', borderRadius: '12px', border: '1px solid #e0e0e0', padding: '20px', marginBottom: '16px' },
  sectionTitle: { fontSize: '16px', fontWeight: 600, marginBottom: '16px', color: '#1a1a2e' },
  riskGrid: { display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: '8px' },
  riskItem: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '8px 12px', backgroundColor: '#f8f9fa', borderRadius: '6px' },
  riskBadge: { padding: '2px 8px', borderRadius: '4px', color: '#fff', fontSize: '12px', fontWeight: 600, textTransform: 'uppercase' },
  riskCount: { fontSize: '18px', fontWeight: 700 },
  limitRow: { display: 'flex', justifyContent: 'space-between', padding: '8px 0', borderBottom: '1px solid #f0f0f0', fontSize: '14px' },
  table: { width: '100%', borderCollapse: 'collapse', fontSize: '13px' },
  th: { textAlign: 'left', padding: '8px', borderBottom: '2px solid #e0e0e0', color: '#666', fontWeight: 600 },
  td: { padding: '8px', borderBottom: '1px solid #f0f0f0' },
  empty: { color: '#666', fontSize: '14px' },
};
