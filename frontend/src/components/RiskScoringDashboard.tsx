import React, { useState, useEffect, useCallback } from 'react';

interface RiskScore {
  pool: string;
  overallScore: number;
  letterGrade: string;
  assetVolatilityScore: number;
  oracleDeviationScore: number;
  poolUtilizationScore: number;
  liquidationHistoryScore: number;
  timestamp: number;
}

interface RiskAlert {
  id: string;
  pool: string;
  severity: 'low' | 'medium' | 'high' | 'critical';
  message: string;
  currentScore: number;
  threshold: number;
  letterGrade: string;
  timestamp: number;
  acknowledged: boolean;
}

interface RiskAnalytics {
  totalPools: number;
  averageScore: number;
  distribution: Record<string, number>;
  alertsActive: number;
  poolsAtRisk: number;
  trendSummary: { improving: number; stable: number; declining: number };
}

const GRADE_COLORS: Record<string, string> = {
  'A+': '#2e7d32', 'A': '#388e3c', 'A-': '#43a047',
  'B+': '#558b2f', 'B': '#689f38', 'B-': '#7cb342',
  'C+': '#f9a825', 'C': '#fb8c00', 'C-': '#ef6c00', 'D': '#d32f2f',
};

const SEVERITY_COLORS = { low: '#558b2f', medium: '#f9a825', high: '#ef6c00', critical: '#d32f2f' };

export const RiskScoringDashboard: React.FC = () => {
  const [scores, setScores] = useState<RiskScore[]>([]);
  const [alerts, setAlerts] = useState<RiskAlert[]>([]);
  const [analytics, setAnalytics] = useState<RiskAnalytics | null>(null);
  const [loading, setLoading] = useState(true);

  const loadData = useCallback(async () => {
    try {
      const [scoresRes, alertsRes, analyticsRes] = await Promise.all([
        fetch('/api/risk-scoring/scores'),
        fetch('/api/risk-scoring/alerts'),
        fetch('/api/risk-scoring/analytics'),
      ]);
      const scoresData = await scoresRes.json();
      const alertsData = await alertsRes.json();
      const analyticsData = await analyticsRes.json();
      if (scoresData.success) setScores(scoresData.scores);
      if (alertsData.success) setAlerts(alertsData.alerts);
      if (analyticsData.success) setAnalytics(analyticsData.analytics);
    } catch (err) {
      console.error('Failed to load risk scoring data', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 30000);
    return () => clearInterval(interval);
  }, [loadData]);

  const acknowledgeAlert = async (alertId: string) => {
    await fetch(`/api/risk-scoring/alerts/${alertId}/acknowledge`, { method: 'POST' });
    loadData();
  };

  if (loading) return <div style={styles.container}><p>Loading risk scores…</p></div>;

  return (
    <div style={styles.container}>
      <h2 style={styles.title}>Risk Scoring Dashboard</h2>

      {analytics && (
        <div style={styles.statsGrid}>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Total Pools</div>
            <div style={styles.statValue}>{analytics.totalPools}</div>
          </div>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Average Score</div>
            <div style={styles.statValue}>{analytics.averageScore}</div>
          </div>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Active Alerts</div>
            <div style={{ ...styles.statValue, color: analytics.alertsActive > 0 ? '#d32f2f' : '#2e7d32' }}>
              {analytics.alertsActive}
            </div>
          </div>
          <div style={styles.statCard}>
            <div style={styles.statLabel}>Pools at Risk</div>
            <div style={styles.statValue}>{analytics.poolsAtRisk}</div>
          </div>
        </div>
      )}

      {alerts.length > 0 && (
        <div style={styles.card}>
          <h3 style={styles.sectionTitle}>Risk Threshold Alerts</h3>
          {alerts.map((alert) => (
            <div key={alert.id} style={{ ...styles.alertRow, borderLeftColor: SEVERITY_COLORS[alert.severity] }}>
              <div>
                <span style={{ ...styles.severityBadge, backgroundColor: SEVERITY_COLORS[alert.severity] }}>
                  {alert.severity}
                </span>
                <span style={styles.alertPool}>{alert.pool}</span>
                <span style={styles.alertGrade}>{alert.letterGrade}</span>
              </div>
              <p style={styles.alertMessage}>{alert.message}</p>
              <button type="button" style={styles.ackButton} onClick={() => acknowledgeAlert(alert.id)}>
                Acknowledge
              </button>
            </div>
          ))}
        </div>
      )}

      <div style={styles.card}>
        <h3 style={styles.sectionTitle}>Real-Time Pool Ratings</h3>
        <table style={styles.table}>
          <thead>
            <tr>
              <th style={styles.th}>Pool</th>
              <th style={styles.th}>Grade</th>
              <th style={styles.th}>Score</th>
              <th style={styles.th}>Volatility</th>
              <th style={styles.th}>Oracle</th>
              <th style={styles.th}>Utilization</th>
              <th style={styles.th}>Liquidations</th>
              <th style={styles.th}>Updated</th>
            </tr>
          </thead>
          <tbody>
            {scores.map((score) => (
              <tr key={score.pool}>
                <td style={styles.td}>{score.pool}</td>
                <td style={styles.td}>
                  <span style={{ ...styles.gradeBadge, backgroundColor: GRADE_COLORS[score.letterGrade] ?? '#666' }}>
                    {score.letterGrade}
                  </span>
                </td>
                <td style={styles.td}>{score.overallScore}</td>
                <td style={styles.td}>{score.assetVolatilityScore}</td>
                <td style={styles.td}>{score.oracleDeviationScore}</td>
                <td style={styles.td}>{score.poolUtilizationScore}</td>
                <td style={styles.td}>{score.liquidationHistoryScore}</td>
                <td style={styles.td}>{new Date(score.timestamp * 1000).toLocaleTimeString()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {analytics && Object.keys(analytics.distribution).length > 0 && (
        <div style={styles.card}>
          <h3 style={styles.sectionTitle}>Score Distribution</h3>
          <div style={styles.distributionGrid}>
            {Object.entries(analytics.distribution).map(([grade, count]) => (
              <div key={grade} style={styles.distributionItem}>
                <span style={{ ...styles.gradeBadge, backgroundColor: GRADE_COLORS[grade] ?? '#666' }}>{grade}</span>
                <span style={styles.distributionCount}>{count} pool{count !== 1 ? 's' : ''}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: { padding: '24px', maxWidth: '1100px', margin: '0 auto', fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif' },
  title: { fontSize: '24px', fontWeight: 600, marginBottom: '24px', color: '#1a1a2e' },
  statsGrid: { display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: '12px', marginBottom: '16px' },
  statCard: { padding: '16px', backgroundColor: '#fff', borderRadius: '8px', border: '1px solid #e0e0e0', textAlign: 'center' },
  statLabel: { fontSize: '11px', color: '#666', textTransform: 'uppercase', fontWeight: 600, marginBottom: '4px' },
  statValue: { fontSize: '22px', fontWeight: 700, color: '#1a1a2e' },
  card: { backgroundColor: '#fff', borderRadius: '12px', border: '1px solid #e0e0e0', padding: '20px', marginBottom: '16px' },
  sectionTitle: { fontSize: '16px', fontWeight: 600, marginBottom: '16px', color: '#1a1a2e' },
  alertRow: { padding: '12px', marginBottom: '8px', backgroundColor: '#fafafa', borderRadius: '6px', borderLeft: '4px solid' },
  severityBadge: { padding: '2px 8px', borderRadius: '4px', color: '#fff', fontSize: '11px', fontWeight: 600, textTransform: 'uppercase', marginRight: '8px' },
  alertPool: { fontWeight: 600, marginRight: '8px' },
  alertGrade: { color: '#666', fontSize: '13px' },
  alertMessage: { fontSize: '13px', color: '#555', margin: '8px 0' },
  ackButton: { padding: '4px 12px', backgroundColor: '#0066ff', color: '#fff', border: 'none', borderRadius: '4px', fontSize: '12px', cursor: 'pointer' },
  table: { width: '100%', borderCollapse: 'collapse', fontSize: '13px' },
  th: { textAlign: 'left', padding: '8px', borderBottom: '2px solid #e0e0e0', color: '#666', fontWeight: 600 },
  td: { padding: '8px', borderBottom: '1px solid #f0f0f0' },
  gradeBadge: { padding: '2px 8px', borderRadius: '4px', color: '#fff', fontSize: '12px', fontWeight: 700 },
  distributionGrid: { display: 'flex', flexWrap: 'wrap', gap: '12px' },
  distributionItem: { display: 'flex', alignItems: 'center', gap: '8px', padding: '8px 12px', backgroundColor: '#f8f9fa', borderRadius: '6px' },
  distributionCount: { fontSize: '14px', fontWeight: 600 },
};
