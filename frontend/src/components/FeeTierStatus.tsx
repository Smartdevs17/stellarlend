import React, { useState, useEffect } from "react";

interface Tier {
  name: string;
  minDeposits: number;
  minBorrowVolume: number;
  minAccountDays: number;
  minLoyalDays: number;
  discountBps: number;
  loyaltyBonusBps: number;
}

interface FeeTierStatusProps {
  current?: Tier;
  next?: Tier;
  progress?: Record<string, number> | null;
  totalSavings?: number;
  effectiveAt?: number;
}

export function FeeTierStatus({
  current: initialCurrent,
  next: initialNext,
  progress: initialProgress,
  totalSavings: initialSavings = 0,
  effectiveAt: initialEffectiveAt = Date.now() + 7 * 86400000,
}: FeeTierStatusProps) {
  const [tiers, setTiers] = useState<Tier[]>([]);
  const [activeTab, setActiveTab] = useState<'status' | 'tiers' | 'calculator'>('status');

  // Calculator / Transparency states
  const [calcAmount, setCalcAmount] = useState<number>(1000);
  const [calcOperation, setCalcOperation] = useState<string>('borrow');
  const [baseFeePercent, setBaseFeePercent] = useState<number>(0.5); // 0.5% base fee
  const [accountAgeDays, setAccountAgeDays] = useState<number>(45);
  const [daysSinceWithdrawal, setDaysSinceWithdrawal] = useState<number>(20);
  const [userDeposits, setUserDeposits] = useState<number>(15000);
  const [userBorrowVolume, setUserBorrowVolume] = useState<number>(6000);
  const [transparencyResult, setTransparencyResult] = useState<any>(null);

  const currentTier: Tier = initialCurrent || {
    name: "Silver",
    minDeposits: 10000,
    minBorrowVolume: 5000,
    minAccountDays: 30,
    minLoyalDays: 14,
    discountBps: 1000,
    loyaltyBonusBps: 100,
  };

  useEffect(() => {
    fetch('/api/fee-tiers/tiers')
      .then((res) => res.json())
      .then((data) => {
        if (data.success && data.data) {
          setTiers(data.data);
        }
      })
      .catch((err) => console.error('Failed to fetch tiers:', err));
  }, []);

  const handleCalculateTransparency = async () => {
    try {
      const res = await fetch('/api/fee-tiers/transparency', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          userAddress: 'user_wallet',
          operation: calcOperation,
          amount: calcAmount,
          baseFeePercent,
          metrics: {
            totalDeposits: userDeposits,
            borrowingVolume: userBorrowVolume,
            accountAgeDays,
            daysSinceWithdrawal,
          },
        }),
      });
      const data = await res.json();
      if (data.success) {
        setTransparencyResult(data.data);
      }
    } catch (err) {
      console.error('Transparency calculation failed:', err);
    }
  };

  return (
    <section aria-labelledby="tier-title" style={styles.container}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}>
        <div>
          <h2 id="tier-title" style={styles.title}>
            Lending Pool Fee Tier & Loyalty System
          </h2>
          <p style={styles.subtitle}>
            Enjoy up to 50% discount on protocol fees with loyalty rewards and transparent pricing.
          </p>
        </div>
        <span style={styles.badge}>{currentTier.name} Tier</span>
      </div>

      {/* Tabs */}
      <div style={styles.tabBar}>
        <button
          onClick={() => setActiveTab('status')}
          style={{ ...styles.tab, backgroundColor: activeTab === 'status' ? '#007bff' : '#f0f0f0', color: activeTab === 'status' ? '#fff' : '#333' }}
        >
          My Tier Status
        </button>
        <button
          onClick={() => setActiveTab('tiers')}
          style={{ ...styles.tab, backgroundColor: activeTab === 'tiers' ? '#007bff' : '#f0f0f0', color: activeTab === 'tiers' ? '#fff' : '#333' }}
        >
          All Fee Tiers
        </button>
        <button
          onClick={() => setActiveTab('calculator')}
          style={{ ...styles.tab, backgroundColor: activeTab === 'calculator' ? '#007bff' : '#f0f0f0', color: activeTab === 'calculator' ? '#fff' : '#333' }}
        >
          Fee Transparency Calculator
        </button>
      </div>

      {/* Tab: Status */}
      {activeTab === 'status' && (
        <div>
          <dl style={styles.grid4}>
            <div style={styles.statBox}>
              <dt style={styles.dt}>Fee Discount</dt>
              <dd style={styles.dd}>{(currentTier.discountBps / 100).toFixed(0)}%</dd>
            </div>
            <div style={styles.statBox}>
              <dt style={styles.dt}>Loyalty Bonus</dt>
              <dd style={styles.dd}>+{(currentTier.loyaltyBonusBps / 100).toFixed(2)}%</dd>
            </div>
            <div style={styles.statBox}>
              <dt style={styles.dt}>Total Lifetime Saved</dt>
              <dd style={styles.dd}>${initialSavings.toFixed(2)}</dd>
            </div>
            <div style={styles.statBox}>
              <dt style={styles.dt}>Next Evaluation</dt>
              <dd style={styles.dd}>{new Date(initialEffectiveAt).toLocaleDateString()}</dd>
            </div>
          </dl>

          {initialNext && initialProgress && (
            <div style={styles.progressSection}>
              <h3 style={{ fontSize: '15px', fontWeight: '600', marginBottom: '12px' }}>
                Progress towards {initialNext.name} Tier
              </h3>
              {Object.entries(initialProgress).map(([metric, value]) => (
                <div key={metric} style={{ marginBottom: '10px' }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '13px', marginBottom: '4px' }}>
                    <span style={{ textTransform: 'capitalize' }}>{metric}</span>
                    <span>{Math.round(value * 100)}%</span>
                  </div>
                  <div style={styles.progressBarBackground}>
                    <div style={{ ...styles.progressBarFill, width: `${Math.min(100, Math.round(value * 100))}%` }} />
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Tab: Tiers Table */}
      {activeTab === 'tiers' && (
        <div style={{ overflowX: 'auto' }}>
          <table style={styles.table}>
            <thead>
              <tr>
                <th style={styles.th}>Tier Name</th>
                <th style={styles.th}>Base Discount</th>
                <th style={styles.th}>Loyalty Bonus</th>
                <th style={styles.th}>Min Deposits</th>
                <th style={styles.th}>Min Borrow Volume</th>
                <th style={styles.th}>Min Account Days</th>
                <th style={styles.th}>Min Loyalty Days</th>
              </tr>
            </thead>
            <tbody>
              {(tiers.length > 0 ? tiers : [currentTier]).map((t) => (
                <tr key={t.name} style={{ backgroundColor: t.name === currentTier.name ? '#e7f1ff' : 'transparent' }}>
                  <td style={styles.td}><strong>{t.name}</strong></td>
                  <td style={{ ...styles.td, color: '#28a745', fontWeight: 'bold' }}>
                    {(t.discountBps / 100).toFixed(0)}%
                  </td>
                  <td style={styles.td}>+{(t.loyaltyBonusBps / 100).toFixed(2)}%</td>
                  <td style={styles.td}>${t.minDeposits.toLocaleString()}</td>
                  <td style={styles.td}>${t.minBorrowVolume.toLocaleString()}</td>
                  <td style={styles.td}>{t.minAccountDays} days</td>
                  <td style={styles.td}>{t.minLoyalDays} days</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Tab: Transparency Calculator */}
      {activeTab === 'calculator' && (
        <div>
          <p style={styles.subtitle}>
            Preview transparent itemized fee breakdowns and see how much you save with loyalty discounts.
          </p>
          <div style={styles.grid3}>
            <div>
              <label style={styles.fieldLabel}>Operation</label>
              <select
                value={calcOperation}
                onChange={(e) => setCalcOperation(e.target.value)}
                style={styles.input}
              >
                <option value="borrow">Borrow Asset</option>
                <option value="flash_loan">Flash Loan</option>
                <option value="withdraw">Withdrawal</option>
                <option value="liquidate">Liquidation</option>
              </select>
            </div>
            <div>
              <label style={styles.fieldLabel}>Transaction Amount ($)</label>
              <input
                type="number"
                value={calcAmount}
                onChange={(e) => setCalcAmount(Number(e.target.value) || 0)}
                style={styles.input}
              />
            </div>
            <div>
              <label style={styles.fieldLabel}>Base Fee Rate (%)</label>
              <input
                type="number"
                step="0.1"
                value={baseFeePercent}
                onChange={(e) => setBaseFeePercent(Number(e.target.value) || 0)}
                style={styles.input}
              />
            </div>
            <div>
              <label style={styles.fieldLabel}>My Total Deposits ($)</label>
              <input
                type="number"
                value={userDeposits}
                onChange={(e) => setUserDeposits(Number(e.target.value) || 0)}
                style={styles.input}
              />
            </div>
            <div>
              <label style={styles.fieldLabel}>My Borrow Volume ($)</label>
              <input
                type="number"
                value={userBorrowVolume}
                onChange={(e) => setUserBorrowVolume(Number(e.target.value) || 0)}
                style={styles.input}
              />
            </div>
            <div>
              <label style={styles.fieldLabel}>Days Without Withdrawal</label>
              <input
                type="number"
                value={daysSinceWithdrawal}
                onChange={(e) => setDaysSinceWithdrawal(Number(e.target.value) || 0)}
                style={styles.input}
              />
            </div>
          </div>

          <button onClick={handleCalculateTransparency} style={{ ...styles.button, marginTop: '16px' }}>
            Calculate Transparent Fee
          </button>

          {transparencyResult && (
            <div style={{ marginTop: '20px', padding: '16px', backgroundColor: '#f8f9fa', borderRadius: '8px', border: '1px solid #e9ecef' }}>
              <h4 style={{ margin: '0 0 8px 0', fontSize: '15px' }}>Itemized Fee Transparency Breakdown</h4>
              <p style={{ fontSize: '13px', color: '#555', marginBottom: '14px' }}>{transparencyResult.explanation}</p>

              <div style={styles.grid3}>
                <div style={styles.statBox}>
                  <dt style={styles.dt}>Nominal Base Fee</dt>
                  <dd style={styles.dd}>${transparencyResult.baseFee.toFixed(2)}</dd>
                </div>
                <div style={styles.statBox}>
                  <dt style={styles.dt}>Total Discount (Tier + Loyalty)</dt>
                  <dd style={{ ...styles.dd, color: '#28a745' }}>
                    -${transparencyResult.totalDiscountAmount.toFixed(2)} ({transparencyResult.effectiveDiscountRatePercent}%)
                  </dd>
                </div>
                <div style={styles.statBox}>
                  <dt style={styles.dt}>Net Fee Payable</dt>
                  <dd style={{ ...styles.dd, color: '#007bff' }}>
                    ${transparencyResult.netFee.toFixed(2)}
                  </dd>
                </div>
              </div>
            </div>
          )}
        </div>
      )}
    </section>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    padding: '24px',
    borderRadius: '12px',
    border: '1px solid #e0e0e0',
    backgroundColor: '#fff',
    fontFamily: 'system-ui, -apple-system, sans-serif',
    maxWidth: '900px',
    margin: '0 auto',
  },
  title: {
    fontSize: '20px',
    fontWeight: 'bold',
    margin: '0 0 4px 0',
  },
  subtitle: {
    fontSize: '13px',
    color: '#666',
    margin: 0,
  },
  badge: {
    padding: '6px 14px',
    borderRadius: '20px',
    backgroundColor: '#007bff',
    color: '#fff',
    fontWeight: 'bold',
    fontSize: '13px',
  },
  tabBar: {
    display: 'flex',
    gap: '8px',
    margin: '16px 0',
  },
  tab: {
    padding: '8px 14px',
    borderRadius: '4px',
    border: 'none',
    cursor: 'pointer',
    fontWeight: '600',
    fontSize: '13px',
  },
  grid4: {
    display: 'grid',
    gridTemplateColumns: 'repeat(4, 1fr)',
    gap: '12px',
    margin: 0,
  },
  grid3: {
    display: 'grid',
    gridTemplateColumns: 'repeat(3, 1fr)',
    gap: '14px',
    margin: 0,
  },
  statBox: {
    padding: '14px',
    backgroundColor: '#f8f9fa',
    borderRadius: '8px',
    border: '1px solid #eee',
  },
  dt: {
    fontSize: '12px',
    color: '#666',
    marginBottom: '4px',
  },
  dd: {
    fontSize: '18px',
    fontWeight: 'bold',
    margin: 0,
    color: '#1a1a1a',
  },
  progressSection: {
    marginTop: '20px',
    padding: '16px',
    backgroundColor: '#f8f9fa',
    borderRadius: '8px',
    border: '1px solid #eee',
  },
  progressBarBackground: {
    height: '8px',
    backgroundColor: '#e9ecef',
    borderRadius: '4px',
    overflow: 'hidden',
  },
  progressBarFill: {
    height: '100%',
    backgroundColor: '#007bff',
    borderRadius: '4px',
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
  },
  td: {
    padding: '10px 12px',
    borderBottom: '1px solid #dee2e6',
  },
  fieldLabel: {
    display: 'block',
    fontSize: '12px',
    fontWeight: '600',
    color: '#495057',
    marginBottom: '4px',
  },
  input: {
    width: '100%',
    padding: '8px 10px',
    borderRadius: '4px',
    border: '1px solid #ced4da',
    fontSize: '13px',
    boxSizing: 'border-box',
  },
  button: {
    padding: '10px 16px',
    backgroundColor: '#007bff',
    color: '#fff',
    borderRadius: '4px',
    border: 'none',
    fontWeight: '600',
    cursor: 'pointer',
    fontSize: '13px',
  },
};
