import React, { useState } from 'react';

interface Position {
  collateral: number;
  debt: number;
  asset: string;
}

interface SimulationResult {
  scenarioName: string;
  currentHealth: number;
  afterScenarioHealth: number;
  isLiquidatable: boolean;
  liquidationPrice: number;
  safetyMargin: number;
  collateralChange?: number;
  debtChange?: number;
}

export const PositionSimulator: React.FC = () => {
  const [position, setPosition] = useState<Position>({
    collateral: 1000,
    debt: 500,
    asset: 'USDC',
  });

  const [simulationResults, setSimulationResults] = useState<SimulationResult[]>([]);
  const [selectedForComparison, setSelectedForComparison] = useState<number[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [activeTab, setActiveTab] = useState<'scenarios' | 'whatif' | 'visualization' | 'complex' | 'history' | 'compare' | 'share'>('whatif');
  const [complexParams, setComplexParams] = useState({ priceDrop: 10, rateIncrease: 5 });
  const [historyDate, setHistoryDate] = useState('2021-05-19');
  const [realTimeAmount, setRealTimeAmount] = useState(0);
  const [realTimeType, setRealTimeType] = useState<'deposit' | 'withdraw' | 'borrow' | 'repay'>('deposit');
  const [realTimeResult, setRealTimeResult] = useState<SimulationResult | null>(null);

  // What-if analysis state
  const [whatIfParams, setWhatIfParams] = useState({
    priceChange: -20,
    deposit: 0,
    withdraw: 0,
    borrow: 0,
    repay: 0,
  });
  const [whatIfResult, setWhatIfResult] = useState<any>(null);

  // Share simulation state
  const [shareLink, setShareLink] = useState<string>('');
  const [shareCopied, setShareCopied] = useState<boolean>(false);
  const [importToken, setImportToken] = useState<string>('');

  const currentHealth = position.debt === 0 ? Infinity : position.collateral / position.debt;

  const handleSimulate = async (endpoint: string, body: Record<string, unknown>) => {
    setIsLoading(true);
    try {
      const response = await fetch(`/api/simulator/${endpoint}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      const data = await response.json();
      if (data.success) {
        const result: SimulationResult = {
          scenarioName: (body.scenarioName as string) || endpoint,
          currentHealth: data.data.current || data.data.initial_health,
          afterScenarioHealth: data.data.after_scenario || data.data.final_health,
          isLiquidatable: data.data.is_liquidatable,
          liquidationPrice: data.data.liquidation_price || 0,
          safetyMargin: data.data.safety_margin || 0,
          collateralChange: data.data.collateral_change,
          debtChange: data.data.debt_change,
        };
        setSimulationResults((prev) => [...prev, result]);
      }
    } catch (error) {
      console.error('Simulation failed:', error);
    }
    setIsLoading(false);
  };

  const handleRunWhatIf = async () => {
    setIsLoading(true);
    try {
      const response = await fetch('/api/simulation/position', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          position,
          scenario: {
            priceChangePercent: whatIfParams.priceChange,
            depositAmount: whatIfParams.deposit,
            withdrawAmount: whatIfParams.withdraw,
            borrowAmount: whatIfParams.borrow,
            repayAmount: whatIfParams.repay,
            scenarioName: `What-If: ${whatIfParams.priceChange}% price, +${whatIfParams.deposit} dep, -${whatIfParams.withdraw} w/d`,
          },
        }),
      });
      const data = await response.json();
      if (data.success) {
        setWhatIfResult(data.data);
      }
    } catch (error) {
      console.error('What-if simulation failed:', error);
    }
    setIsLoading(false);
  };

  const handleShareSimulation = async () => {
    setIsLoading(true);
    try {
      const response = await fetch('/api/simulation/share', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          position,
          scenario: {
            priceChangePercent: whatIfParams.priceChange,
            depositAmount: whatIfParams.deposit,
            withdrawAmount: whatIfParams.withdraw,
            borrowAmount: whatIfParams.borrow,
            repayAmount: whatIfParams.repay,
          },
          result: whatIfResult,
        }),
      });
      const data = await response.json();
      if (data.success) {
        const link = `${window.location.origin}/simulation/view/${data.data.share_token}`;
        setShareLink(link);
      }
    } catch (error) {
      console.error('Sharing failed:', error);
    }
    setIsLoading(false);
  };

  const handleLoadShared = async () => {
    if (!importToken) return;
    setIsLoading(true);
    try {
      const response = await fetch(`/api/simulation/share/${importToken.trim()}`);
      const data = await response.json();
      if (data.success && data.data) {
        setPosition(data.data.position);
        if (data.data.scenario) {
          setWhatIfParams({
            priceChange: data.data.scenario.priceChangePercent ?? 0,
            deposit: data.data.scenario.depositAmount ?? 0,
            withdraw: data.data.scenario.withdrawAmount ?? 0,
            borrow: data.data.scenario.borrowAmount ?? 0,
            repay: data.data.scenario.repayAmount ?? 0,
          });
        }
        if (data.data.result) {
          setWhatIfResult(data.data.result);
        }
        setActiveTab('whatif');
      }
    } catch (error) {
      console.error('Load shared simulation failed:', error);
    }
    setIsLoading(false);
  };

  const getHealthColor = (hf: number) => {
    if (hf >= 1.5) return '#28a745'; // Safe Green
    if (hf >= 1.2) return '#ffc107'; // Moderate Yellow
    if (hf >= 1.05) return '#fd7e14'; // Warning Orange
    return '#dc3545'; // Critical Red
  };

  return (
    <div style={styles.container}>
      <h2 style={styles.title}>Lending Pool Position Health Simulator</h2>
      <p style={styles.subtitle}>
        Simulate position safety, scenario modeling, what-if stress tests, and share simulations.
      </p>

      {/* Position Overview Card */}
      <div style={styles.card}>
        <h3 style={styles.cardTitle}>Current Position</h3>
        <div style={styles.grid3}>
          <div>
            <label style={styles.fieldLabel}>Collateral ({position.asset})</label>
            <input
              type="number"
              value={position.collateral}
              onChange={(e) => setPosition((prev) => ({ ...prev, collateral: parseFloat(e.target.value) || 0 }))}
              style={styles.input}
            />
          </div>
          <div>
            <label style={styles.fieldLabel}>Borrowed Debt ({position.asset})</label>
            <input
              type="number"
              value={position.debt}
              onChange={(e) => setPosition((prev) => ({ ...prev, debt: parseFloat(e.target.value) || 0 }))}
              style={styles.input}
            />
          </div>
          <div>
            <label style={styles.fieldLabel}>Health Factor</label>
            <div
              style={{
                ...styles.healthFactor,
                backgroundColor: getHealthColor(currentHealth),
              }}
            >
              {currentHealth === Infinity ? '∞ (No Debt)' : `${currentHealth.toFixed(2)}x`}
            </div>
          </div>
        </div>
      </div>

      {/* Navigation Tabs */}
      <div style={styles.tabBar}>
        {(['whatif', 'visualization', 'scenarios', 'complex', 'history', 'compare', 'share'] as const).map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            style={{
              ...styles.tab,
              backgroundColor: activeTab === tab ? '#007bff' : '#f0f0f0',
              color: activeTab === tab ? 'white' : '#333',
            }}
          >
            {tab === 'whatif'
              ? 'What-If Analysis'
              : tab === 'visualization'
              ? 'Health Visualization'
              : tab === 'scenarios'
              ? 'Scenarios'
              : tab === 'complex'
              ? 'Complex'
              : tab === 'history'
              ? 'Historical'
              : tab === 'compare'
              ? 'Compare'
              : 'Share Simulation'}
          </button>
        ))}
      </div>

      {/* Tab: What-If Analysis */}
      {activeTab === 'whatif' && (
        <div style={styles.section}>
          <h3>What-If Scenario Modeling</h3>
          <p style={styles.hint}>
            Simulate collateral price swings, emergency deposits, or planned withdrawals to test liquidation immunity.
          </p>
          <div style={styles.grid2}>
            <div>
              <label style={styles.fieldLabel}>Price Change: {whatIfParams.priceChange}%</label>
              <input
                type="range"
                min="-80"
                max="80"
                step="5"
                value={whatIfParams.priceChange}
                onChange={(e) => setWhatIfParams((p) => ({ ...p, priceChange: Number(e.target.value) }))}
                style={{ width: '100%' }}
              />
            </div>
            <div>
              <label style={styles.fieldLabel}>Simulated Deposit (+Collateral)</label>
              <input
                type="number"
                value={whatIfParams.deposit}
                onChange={(e) => setWhatIfParams((p) => ({ ...p, deposit: Number(e.target.value) || 0 }))}
                style={styles.input}
              />
            </div>
            <div>
              <label style={styles.fieldLabel}>Simulated Withdrawal (-Collateral)</label>
              <input
                type="number"
                value={whatIfParams.withdraw}
                onChange={(e) => setWhatIfParams((p) => ({ ...p, withdraw: Number(e.target.value) || 0 }))}
                style={styles.input}
              />
            </div>
            <div>
              <label style={styles.fieldLabel}>Simulated Repayment (-Debt)</label>
              <input
                type="number"
                value={whatIfParams.repay}
                onChange={(e) => setWhatIfParams((p) => ({ ...p, repay: Number(e.target.value) || 0 }))}
                style={styles.input}
              />
            </div>
          </div>

          <button onClick={handleRunWhatIf} disabled={isLoading} style={{ ...styles.button, marginTop: '16px' }}>
            {isLoading ? 'Simulating...' : 'Run What-If Analysis'}
          </button>

          {whatIfResult && (
            <div style={{ ...styles.resultCard, marginTop: '20px' }}>
              <div style={styles.resultHeader}>
                <h4>Simulation Outcome: {whatIfResult.scenario_name}</h4>
                <span
                  style={{
                    ...styles.statusBadge,
                    backgroundColor: whatIfResult.is_liquidatable ? '#dc3545' : '#28a745',
                  }}
                >
                  {whatIfResult.is_liquidatable ? 'LIQUIDATABLE' : 'HEALTHY'}
                </span>
              </div>
              <div style={styles.grid2}>
                <div>
                  <strong>Simulated Health Factor:</strong>{' '}
                  <span style={{ color: getHealthColor(whatIfResult.simulated_position.health_factor), fontWeight: 'bold' }}>
                    {whatIfResult.simulated_position.health_factor}x
                  </span>
                </div>
                <div>
                  <strong>Liquidation Price Drop:</strong> -{whatIfResult.liquidation_price_drop_percent}%
                </div>
                <div>
                  <strong>Max Safe Withdrawal:</strong> {whatIfResult.max_withdrawable_amount} {position.asset}
                </div>
                <div>
                  <strong>Max Safe Additional Borrow:</strong> {whatIfResult.max_borrowable_amount} {position.asset}
                </div>
                <div>
                  <strong>Simulated Collateral:</strong> {whatIfResult.simulated_position.collateral} {position.asset}
                </div>
                <div>
                  <strong>Simulated Debt:</strong> {whatIfResult.simulated_position.debt} {position.asset}
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Tab: Health Visualization */}
      {activeTab === 'visualization' && (
        <div style={styles.section}>
          <h3>Position Health Sensitivity Curve</h3>
          <p style={styles.hint}>Visual health factor response to collateral asset price movements (-60% to +40%).</p>
          <div style={{ background: '#fff', padding: '16px', borderRadius: '8px', border: '1px solid #e0e0e0' }}>
            <svg viewBox="0 0 500 200" style={{ width: '100%', height: 'auto' }}>
              {/* Liquidation threshold line at HF = 1.0 */}
              <line x1="50" y1="140" x2="480" y2="140" stroke="#dc3545" strokeDasharray="4" strokeWidth="2" />
              <text x="55" y="135" fill="#dc3545" fontSize="10">Liquidation Threshold (1.0x)</text>

              {/* Curve points calculation */}
              {(() => {
                const points = [-60, -40, -20, 0, 20, 40].map((drop, idx) => {
                  const effectiveCollat = position.collateral * (1 + drop / 100);
                  const hf = position.debt === 0 ? 3.0 : Math.min(3.0, effectiveCollat / position.debt);
                  const x = 50 + idx * 80;
                  const y = 180 - (hf / 2.5) * 120;
                  return { x, y, hf, drop };
                });

                const pathD = points.reduce((acc, p, i) => `${acc} ${i === 0 ? 'M' : 'L'} ${p.x} ${p.y}`, '');

                return (
                  <>
                    <path d={pathD} fill="none" stroke="#007bff" strokeWidth="3" />
                    {points.map((p, i) => (
                      <g key={i}>
                        <circle cx={p.x} cy={p.y} r="5" fill={p.hf < 1.0 ? '#dc3545' : '#28a745'} />
                        <text x={p.x - 15} y="195" fontSize="10" fill="#666">
                          {p.drop > 0 ? `+${p.drop}%` : `${p.drop}%`}
                        </text>
                        <text x={p.x - 10} y={p.y - 8} fontSize="9" fontWeight="bold" fill="#333">
                          {p.hf.toFixed(1)}x
                        </text>
                      </g>
                    ))}
                  </>
                );
              })()}
            </svg>
          </div>
        </div>
      )}

      {/* Tab: Basic Scenarios */}
      {activeTab === 'scenarios' && (
        <div style={styles.scenariosSection}>
          <div style={styles.buttonsGrid}>
            <button onClick={() => handleSimulate('price-drop', { position, priceDropPercent: 20, scenarioName: '20% Price Drop' })} disabled={isLoading} style={styles.button}>
              20% Price Drop
            </button>
            <button onClick={() => handleSimulate('rate-increase', { position, rateIncreasePercent: 5, scenarioName: '5% Rate Increase' })} disabled={isLoading} style={styles.button}>
              5% Rate Increase
            </button>
            <button onClick={() => handleSimulate('deposit', { position, depositAmount: 100, scenarioName: 'Add 100 Collateral' })} disabled={isLoading} style={styles.button}>
              Add Collateral
            </button>
            <button onClick={() => handleSimulate('repay', { position, repaymentAmount: 50, scenarioName: 'Repay 50 Debt' })} disabled={isLoading} style={styles.button}>
              Repay Debt
            </button>
          </div>
        </div>
      )}

      {/* Tab: Complex */}
      {activeTab === 'complex' && (
        <div style={styles.section}>
          <h3>Complex Scenario (Price Drop + Rate Increase)</h3>
          <div style={styles.complexInputs}>
            <div>
              <label style={styles.fieldLabel}>Price Drop %</label>
              <input
                type="number"
                value={complexParams.priceDrop}
                onChange={(e) => setComplexParams((prev) => ({ ...prev, priceDrop: parseFloat(e.target.value) || 0 }))}
                style={styles.input}
              />
            </div>
            <div>
              <label style={styles.fieldLabel}>Rate Increase %</label>
              <input
                type="number"
                value={complexParams.rateIncrease}
                onChange={(e) => setComplexParams((prev) => ({ ...prev, rateIncrease: parseFloat(e.target.value) || 0 }))}
                style={styles.input}
              />
            </div>
            <button
              onClick={() =>
                handleSimulate('complex', {
                  position,
                  priceDropPercent: complexParams.priceDrop,
                  rateIncreasePercent: complexParams.rateIncrease,
                  scenarioName: `Complex: ${complexParams.priceDrop}% drop + ${complexParams.rateIncrease}% rate`,
                })
              }
              disabled={isLoading}
              style={{ ...styles.button, alignSelf: 'flex-end' }}
            >
              Run Complex Simulation
            </button>
          </div>
        </div>
      )}

      {/* Tab: Simulation Sharing */}
      {activeTab === 'share' && (
        <div style={styles.section}>
          <h3>Share & Load Simulation</h3>
          <p style={styles.hint}>Export your simulation scenario as a persistent link or import a scenario from a teammate.</p>
          
          <div style={{ marginBottom: '20px' }}>
            <h4>Generate Shareable Link</h4>
            <button onClick={handleShareSimulation} disabled={isLoading} style={styles.button}>
              {isLoading ? 'Generating Link...' : 'Share Current Scenario'}
            </button>
            {shareLink && (
              <div style={{ marginTop: '12px' }}>
                <input type="text" readOnly value={shareLink} style={styles.input} />
                <button
                  onClick={() => {
                    navigator.clipboard.writeText(shareLink);
                    setShareCopied(true);
                    setTimeout(() => setShareCopied(false), 2000);
                  }}
                  style={{ ...styles.clearButton, marginTop: '8px' }}
                >
                  {shareCopied ? 'Copied!' : 'Copy to Clipboard'}
                </button>
              </div>
            )}
          </div>

          <hr style={{ margin: '20px 0', border: 'none', borderTop: '1px solid #eee' }} />

          <div>
            <h4>Load Shared Scenario</h4>
            <div style={{ display: 'flex', gap: '8px' }}>
              <input
                type="text"
                placeholder="Paste Simulation Token (e.g. e4a90f1b2c)"
                value={importToken}
                onChange={(e) => setImportToken(e.target.value)}
                style={styles.input}
              />
              <button onClick={handleLoadShared} disabled={isLoading || !importToken} style={styles.button}>
                Load
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    maxWidth: '900px',
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
  card: {
    padding: '20px',
    backgroundColor: '#f8f9fa',
    borderRadius: '8px',
    border: '1px solid #e9ecef',
    marginBottom: '20px',
  },
  cardTitle: {
    fontSize: '16px',
    fontWeight: '600',
    marginBottom: '12px',
  },
  grid3: {
    display: 'grid',
    gridTemplateColumns: 'repeat(3, 1fr)',
    gap: '16px',
  },
  grid2: {
    display: 'grid',
    gridTemplateColumns: 'repeat(2, 1fr)',
    gap: '16px',
  },
  fieldLabel: {
    display: 'block',
    fontSize: '13px',
    fontWeight: '600',
    color: '#495057',
    marginBottom: '6px',
  },
  input: {
    width: '100%',
    padding: '8px 12px',
    borderRadius: '4px',
    border: '1px solid #ced4da',
    fontSize: '14px',
    boxSizing: 'border-box',
  },
  healthFactor: {
    padding: '8px 12px',
    borderRadius: '4px',
    color: '#fff',
    fontWeight: 'bold',
    textAlign: 'center',
    fontSize: '16px',
  },
  tabBar: {
    display: 'flex',
    gap: '8px',
    marginBottom: '20px',
    overflowX: 'auto',
  },
  tab: {
    padding: '8px 16px',
    borderRadius: '4px',
    border: 'none',
    cursor: 'pointer',
    fontWeight: '600',
    fontSize: '13px',
    whiteSpace: 'nowrap',
  },
  section: {
    padding: '20px',
    backgroundColor: '#fff',
    borderRadius: '8px',
    border: '1px solid #e0e0e0',
    marginBottom: '20px',
  },
  hint: {
    fontSize: '13px',
    color: '#666',
    marginBottom: '16px',
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
  scenariosSection: {
    marginBottom: '20px',
  },
  buttonsGrid: {
    display: 'grid',
    gridTemplateColumns: 'repeat(2, 1fr)',
    gap: '12px',
  },
  complexInputs: {
    display: 'flex',
    gap: '12px',
    alignItems: 'flex-end',
    flexWrap: 'wrap',
  },
  resultCard: {
    padding: '16px',
    backgroundColor: '#f8f9fa',
    borderRadius: '6px',
    border: '1px solid #dee2e6',
  },
  resultHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '12px',
  },
  statusBadge: {
    padding: '4px 10px',
    borderRadius: '4px',
    color: '#fff',
    fontSize: '12px',
    fontWeight: 'bold',
  },
  clearButton: {
    padding: '6px 12px',
    backgroundColor: '#e9ecef',
    border: '1px solid #ced4da',
    borderRadius: '4px',
    cursor: 'pointer',
    fontSize: '12px',
  },
};
