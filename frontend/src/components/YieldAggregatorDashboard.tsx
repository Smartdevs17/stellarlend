import React, { useState, useEffect, useCallback } from 'react';

interface LendingPoolYield {
  poolId: string;
  poolName: string;
  asset: string;
  protocol: string;
  supplyApy: number;
  borrowApy: number;
  rewardApy: number;
  netApy: number;
  tvl: number;
  utilizationRate: number;
  riskScore: number;
  historicalAvg7d: number;
  historicalAvg30d: number;
}

interface RouteAllocation {
  poolId: string;
  poolName: string;
  protocol: string;
  allocatedAmount: number;
  allocationPercent: number;
  poolInitialApy: number;
  expectedMarginalApy: number;
  projectedAnnualEarnings: number;
}

interface BestRateRouteResult {
  asset: string;
  depositAmount: number;
  strategy: string;
  blendedApy: number;
  projectedAnnualEarnings: number;
  allocations: RouteAllocation[];
  singlePoolBestApy: number;
  recommendation: string;
}

export const YieldAggregatorDashboard: React.FC = () => {
  const [pools, setPools] = useState<LendingPoolYield[]>([]);
  const [selectedAsset, setSelectedAsset] = useState<string>('USDC');
  const [depositAmount, setDepositAmount] = useState<number>(10000);
  const [strategy, setStrategy] = useState<'highest_yield' | 'balanced_risk' | 'gas_optimized'>('highest_yield');
  const [routeResult, setRouteResult] = useState<BestRateRouteResult | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [activeTab, setActiveTab] = useState<'router' | 'pools' | 'compare' | 'alerts'>('router');

  // Yield Alert states
  const [alertTargetApy, setAlertTargetApy] = useState<number>(10);
  const [alertCondition, setAlertCondition] = useState<'above' | 'below'>('above');
  const [alertSuccessMessage, setAlertSuccessMessage] = useState<string>('');

  const fetchPools = useCallback(async () => {
    try {
      const res = await fetch(`/api/yield-aggregator/pools?asset=${selectedAsset}`);
      const data = await res.json();
      if (data.success) {
        setPools(data.data);
      }
    } catch (err) {
      console.error('Failed to fetch pools:', err);
    }
  }, [selectedAsset]);

  useEffect(() => {
    fetchPools();
  }, [fetchPools]);

  const handleFindRoute = async () => {
    setIsLoading(true);
    try {
      const res = await fetch('/api/yield-aggregator/route', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          asset: selectedAsset,
          depositAmount,
          strategy,
        }),
      });
      const data = await res.json();
      if (data.success) {
        setRouteResult(data.data);
      }
    } catch (err) {
      console.error('Failed to find route:', err);
    }
    setIsLoading(false);
  };

  const handleCreateAlert = async () => {
    try {
      const res = await fetch('/api/yield-aggregator/alerts', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          userId: 'user_active_wallet',
          asset: selectedAsset,
          targetApy: alertTargetApy / 100,
          condition: alertCondition,
        }),
      });
      const data = await res.json();
      if (data.success) {
        setAlertSuccessMessage(`Alert created! We will notify you when ${selectedAsset} APY goes ${alertCondition} ${alertTargetApy}%.`);
        setTimeout(() => setAlertSuccessMessage(''), 4000);
      }
    } catch (err) {
      console.error('Failed to create alert:', err);
    }
  };

  return (
    <div style={styles.container}>
      <h2 style={styles.title}>Lending Pool Yield Aggregator & Best-Rate Router</h2>
      <p style={styles.subtitle}>
        Automatically aggregate yields across Stellar lending pools and optimize deposits with algorithmic rate routing.
      </p>

      {/* Tabs */}
      <div style={styles.tabBar}>
        {(['router', 'pools', 'compare', 'alerts'] as const).map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            style={{
              ...styles.tab,
              backgroundColor: activeTab === tab ? '#007bff' : '#f0f0f0',
              color: activeTab === tab ? 'white' : '#333',
            }}
          >
            {tab === 'router'
              ? 'Best-Rate Router'
              : tab === 'pools'
              ? 'Aggregated Pools'
              : tab === 'compare'
              ? 'Yield Comparison'
              : 'Yield Alerts'}
          </button>
        ))}
      </div>

      {/* Best-Rate Router Tab */}
      {activeTab === 'router' && (
        <div style={styles.card}>
          <h3 style={styles.cardTitle}>Find Best-Rate Yield Route</h3>
          <p style={styles.hint}>
            Our convex routing algorithm accounts for pool liquidity and rate degradation to allocate capital optimally.
          </p>

          <div style={styles.grid3}>
            <div>
              <label style={styles.fieldLabel}>Deposit Asset</label>
              <select
                value={selectedAsset}
                onChange={(e) => setSelectedAsset(e.target.value)}
                style={styles.select}
              >
                <option value="USDC">USDC</option>
                <option value="XLM">XLM</option>
                <option value="EURC">EURC</option>
                <option value="BTC">BTC</option>
              </select>
            </div>
            <div>
              <label style={styles.fieldLabel}>Deposit Amount</label>
              <input
                type="number"
                value={depositAmount}
                onChange={(e) => setDepositAmount(Number(e.target.value) || 0)}
                style={styles.input}
              />
            </div>
            <div>
              <label style={styles.fieldLabel}>Optimization Strategy</label>
              <select
                value={strategy}
                onChange={(e) => setStrategy(e.target.value as any)}
                style={styles.select}
              >
                <option value="highest_yield">Highest Net Yield (Maximize APY)</option>
                <option value="balanced_risk">Balanced Risk (Adjusted for Pool Score)</option>
                <option value="gas_optimized">Gas-Optimized (Single Route)</option>
              </select>
            </div>
          </div>

          <button onClick={handleFindRoute} disabled={isLoading} style={{ ...styles.button, marginTop: '16px' }}>
            {isLoading ? 'Routing...' : 'Calculate Optimal Route'}
          </button>

          {routeResult && (
            <div style={{ marginTop: '24px', padding: '16px', backgroundColor: '#fff', borderRadius: '8px', border: '1px solid #e0e0e0' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '12px' }}>
                <h4 style={{ margin: 0, fontSize: '16px' }}>Recommended Multi-Pool Route</h4>
                <span style={{ fontSize: '18px', fontWeight: 'bold', color: '#28a745' }}>
                  Blended APY: {(routeResult.blendedApy * 100).toFixed(2)}%
                </span>
              </div>
              <p style={{ fontSize: '13px', color: '#555', marginBottom: '16px' }}>{routeResult.recommendation}</p>

              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: '12px', marginBottom: '16px' }}>
                <div style={styles.metricCard}>
                  <div style={styles.metricLabel}>Projected 1Y Earnings</div>
                  <div style={styles.metricValue}>
                    ${routeResult.projectedAnnualEarnings.toLocaleString()} {selectedAsset}
                  </div>
                </div>
                <div style={styles.metricCard}>
                  <div style={styles.metricLabel}>Single-Pool Benchmark APY</div>
                  <div style={styles.metricValue}>{(routeResult.singlePoolBestApy * 100).toFixed(2)}%</div>
                </div>
              </div>

              <h5 style={{ margin: '12px 0 8px 0', fontSize: '14px' }}>Route Allocations</h5>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                {routeResult.allocations.map((alloc) => (
                  <div
                    key={alloc.poolId}
                    style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      padding: '10px 14px',
                      backgroundColor: '#f8f9fa',
                      borderRadius: '6px',
                      border: '1px solid #eee',
                    }}
                  >
                    <div>
                      <strong>{alloc.poolName}</strong>{' '}
                      <span style={{ fontSize: '12px', color: '#777' }}>({alloc.protocol})</span>
                      <div style={{ fontSize: '12px', color: '#555' }}>
                        Allocation: {alloc.allocationPercent}% (${alloc.allocatedAmount.toLocaleString()})
                      </div>
                    </div>
                    <div style={{ textAlign: 'right' }}>
                      <div style={{ fontWeight: 'bold', color: '#007bff' }}>
                        {(alloc.expectedMarginalApy * 100).toFixed(2)}% APY
                      </div>
                      <div style={{ fontSize: '12px', color: '#28a745' }}>
                        +${alloc.projectedAnnualEarnings.toFixed(2)}/yr
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* Aggregated Pools Tab */}
      {activeTab === 'pools' && (
        <div style={styles.card}>
          <h3 style={styles.cardTitle}>Live Aggregated Lending Pools</h3>
          <div style={{ overflowX: 'auto' }}>
            <table style={styles.table}>
              <thead>
                <tr>
                  <th style={styles.th}>Pool / Venue</th>
                  <th style={styles.th}>Asset</th>
                  <th style={styles.th}>Base APY</th>
                  <th style={styles.th}>Rewards</th>
                  <th style={styles.th}>Net APY</th>
                  <th style={styles.th}>TVL</th>
                  <th style={styles.th}>Utilization</th>
                  <th style={styles.th}>Risk</th>
                </tr>
              </thead>
              <tbody>
                {pools.map((p) => (
                  <tr key={p.poolId}>
                    <td style={styles.td}>
                      <strong>{p.poolName}</strong>
                      <div style={{ fontSize: '11px', color: '#777' }}>{p.protocol}</div>
                    </td>
                    <td style={styles.td}>{p.asset}</td>
                    <td style={styles.td}>{(p.supplyApy * 100).toFixed(2)}%</td>
                    <td style={styles.td}>+{(p.rewardApy * 100).toFixed(2)}%</td>
                    <td style={{ ...styles.td, fontWeight: 'bold', color: '#28a745' }}>
                      {(p.netApy * 100).toFixed(2)}%
                    </td>
                    <td style={styles.td}>${(p.tvl / 1_000_000).toFixed(1)}M</td>
                    <td style={styles.td}>{(p.utilizationRate * 100).toFixed(0)}%</td>
                    <td style={styles.td}>
                      <span
                        style={{
                          padding: '2px 8px',
                          borderRadius: '4px',
                          backgroundColor: p.riskScore <= 2 ? '#d4edda' : p.riskScore <= 4 ? '#fff3cd' : '#f8d7da',
                          color: p.riskScore <= 2 ? '#155724' : p.riskScore <= 4 ? '#856404' : '#721c24',
                          fontSize: '12px',
                        }}
                      >
                        Risk {p.riskScore}/10
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Yield Comparison Tab */}
      {activeTab === 'compare' && (
        <div style={styles.card}>
          <h3 style={styles.cardTitle}>Side-by-Side Yield Comparison ({selectedAsset})</h3>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: '16px' }}>
            {pools.map((p) => (
              <div key={p.poolId} style={styles.compareCard}>
                <h4 style={{ margin: '0 0 4px 0', fontSize: '15px' }}>{p.poolName}</h4>
                <div style={{ fontSize: '12px', color: '#666', marginBottom: '12px' }}>{p.protocol}</div>
                <div style={{ fontSize: '24px', fontWeight: 'bold', color: '#28a745', marginBottom: '8px' }}>
                  {(p.netApy * 100).toFixed(2)}% APY
                </div>
                <div style={{ fontSize: '12px', color: '#555', lineHeight: '1.6' }}>
                  <div>7-Day Avg: {(p.historicalAvg7d * 100).toFixed(2)}%</div>
                  <div>30-Day Avg: {(p.historicalAvg30d * 100).toFixed(2)}%</div>
                  <div>TVL: ${(p.tvl / 1_000_000).toFixed(1)}M</div>
                  <div>Utilization: {(p.utilizationRate * 100).toFixed(0)}%</div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Yield Alerts Tab */}
      {activeTab === 'alerts' && (
        <div style={styles.card}>
          <h3 style={styles.cardTitle}>Set Yield Alerts</h3>
          <p style={styles.hint}>Receive alerts when APY cross specific thresholds or when better routing opportunities emerge.</p>
          
          <div style={styles.grid3}>
            <div>
              <label style={styles.fieldLabel}>Asset</label>
              <select value={selectedAsset} onChange={(e) => setSelectedAsset(e.target.value)} style={styles.select}>
                <option value="USDC">USDC</option>
                <option value="XLM">XLM</option>
                <option value="EURC">EURC</option>
              </select>
            </div>
            <div>
              <label style={styles.fieldLabel}>Condition</label>
              <select value={alertCondition} onChange={(e) => setAlertCondition(e.target.value as any)} style={styles.select}>
                <option value="above">Net APY Rises Above</option>
                <option value="below">Net APY Drops Below</option>
              </select>
            </div>
            <div>
              <label style={styles.fieldLabel}>Target APY (%)</label>
              <input
                type="number"
                step="0.5"
                value={alertTargetApy}
                onChange={(e) => setAlertTargetApy(Number(e.target.value) || 0)}
                style={styles.input}
              />
            </div>
          </div>

          <button onClick={handleCreateAlert} style={{ ...styles.button, marginTop: '16px' }}>
            Create Alert Subscription
          </button>

          {alertSuccessMessage && (
            <div style={{ marginTop: '14px', padding: '10px 14px', backgroundColor: '#d4edda', color: '#155724', borderRadius: '4px' }}>
              {alertSuccessMessage}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    maxWidth: '960px',
    margin: '0 auto',
    padding: '24px',
    fontFamily: 'system-ui, -apple-system, sans-serif',
  },
  title: {
    fontSize: '24px',
    fontWeight: 'bold',
    marginBottom: '6px',
    color: '#1a1a1a',
  },
  subtitle: {
    fontSize: '14px',
    color: '#666',
    marginBottom: '20px',
  },
  tabBar: {
    display: 'flex',
    gap: '8px',
    marginBottom: '20px',
  },
  tab: {
    padding: '8px 16px',
    borderRadius: '4px',
    border: 'none',
    cursor: 'pointer',
    fontWeight: '600',
    fontSize: '13px',
  },
  card: {
    padding: '20px',
    backgroundColor: '#f8f9fa',
    borderRadius: '8px',
    border: '1px solid #e9ecef',
  },
  cardTitle: {
    fontSize: '16px',
    fontWeight: '600',
    marginBottom: '8px',
  },
  hint: {
    fontSize: '13px',
    color: '#666',
    marginBottom: '16px',
  },
  grid3: {
    display: 'grid',
    gridTemplateColumns: 'repeat(3, 1fr)',
    gap: '16px',
  },
  fieldLabel: {
    display: 'block',
    fontSize: '13px',
    fontWeight: '600',
    color: '#495057',
    marginBottom: '6px',
  },
  select: {
    width: '100%',
    padding: '8px 12px',
    borderRadius: '4px',
    border: '1px solid #ced4da',
    fontSize: '14px',
  },
  input: {
    width: '100%',
    padding: '8px 12px',
    borderRadius: '4px',
    border: '1px solid #ced4da',
    fontSize: '14px',
    boxSizing: 'border-box',
  },
  button: {
    padding: '10px 18px',
    backgroundColor: '#007bff',
    color: '#fff',
    borderRadius: '4px',
    border: 'none',
    fontWeight: '600',
    cursor: 'pointer',
    fontSize: '14px',
  },
  metricCard: {
    padding: '12px',
    backgroundColor: '#f8f9fa',
    borderRadius: '6px',
    border: '1px solid #eee',
  },
  metricLabel: {
    fontSize: '12px',
    color: '#666',
    marginBottom: '4px',
  },
  metricValue: {
    fontSize: '18px',
    fontWeight: 'bold',
    color: '#1a1a1a',
  },
  table: {
    width: '100%',
    borderCollapse: 'collapse',
    fontSize: '13px',
  },
  th: {
    textAlign: 'left',
    padding: '10px 12px',
    borderBottom: '2px solid #dee2e6',
    color: '#495057',
    fontWeight: '600',
  },
  td: {
    padding: '10px 12px',
    borderBottom: '1px solid #dee2e6',
    verticalAlign: 'middle',
  },
  compareCard: {
    padding: '16px',
    backgroundColor: '#fff',
    borderRadius: '8px',
    border: '1px solid #e0e0e0',
  },
};
