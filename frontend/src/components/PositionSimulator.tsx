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
}

export const PositionSimulator: React.FC = () => {
  const [position, setPosition] = useState<Position>({
    collateral: 1000,
    debt: 500,
    asset: 'USDC',
  });

  const [simulationResults, setSimulationResults] = useState<SimulationResult[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const handleSimulatePriceDrop = async () => {
    setIsLoading(true);
    try {
      const response = await fetch('/api/simulator/price-drop', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          position,
          priceDropPercent: 20,
        }),
      });
      const result: SimulationResult = await response.json();
      setSimulationResults(prev => [...prev, result]);
    } catch (error) {
      console.error('Simulation failed:', error);
    }
    setIsLoading(false);
  };

  const handleSimulateRateIncrease = async () => {
    setIsLoading(true);
    try {
      const response = await fetch('/api/simulator/rate-increase', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          position,
          rateIncreasePercent: 5,
        }),
      });
      const result: SimulationResult = await response.json();
      setSimulationResults(prev => [...prev, result]);
    } catch (error) {
      console.error('Simulation failed:', error);
    }
    setIsLoading(false);
  };

  const handleAddCollateral = async () => {
    setIsLoading(true);
    try {
      const response = await fetch('/api/simulator/deposit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          position,
          depositAmount: 100,
        }),
      });
      const result: SimulationResult = await response.json();
      setSimulationResults(prev => [...prev, result]);
    } catch (error) {
      console.error('Simulation failed:', error);
    }
    setIsLoading(false);
  };

  const handleRepayDebt = async () => {
    setIsLoading(true);
    try {
      const response = await fetch('/api/simulator/repay', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          position,
          repaymentAmount: 50,
        }),
      });
      const result: SimulationResult = await response.json();
      setSimulationResults(prev => [...prev, result]);
    } catch (error) {
      console.error('Simulation failed:', error);
    }
    setIsLoading(false);
  };

  const getHealthColor = (health: number): string => {
    if (health >= 2) return '#28a745'; // Green
    if (health >= 1.5) return '#ffc107'; // Yellow
    if (health >= 1) return '#fd7e14'; // Orange
    return '#dc3545'; // Red
  };

  const currentHealth = position.collateral / position.debt;

  return (
    <div style={styles.container}>
      <h2>Position Health Simulator</h2>

      <div style={styles.positionCard}>
        <h3>Current Position</h3>
        <div style={styles.positionDetails}>
          <div>
            <label>Collateral:</label>
            <input
              type="number"
              value={position.collateral}
              onChange={e => setPosition(prev => ({ ...prev, collateral: parseFloat(e.target.value) }))}
              style={styles.input}
            />
          </div>
          <div>
            <label>Debt:</label>
            <input
              type="number"
              value={position.debt}
              onChange={e => setPosition(prev => ({ ...prev, debt: parseFloat(e.target.value) }))}
              style={styles.input}
            />
          </div>
          <div>
            <label>Health Factor:</label>
            <div
              style={{
                ...styles.healthFactor,
                backgroundColor: getHealthColor(currentHealth),
              }}
            >
              {currentHealth.toFixed(2)}
            </div>
          </div>
        </div>
      </div>

      <div style={styles.scenariosSection}>
        <h3>Simulation Scenarios</h3>
        <div style={styles.buttonsGrid}>
          <button
            onClick={handleSimulatePriceDrop}
            disabled={isLoading}
            style={styles.button}
          >
            📉 20% Price Drop
          </button>
          <button
            onClick={handleSimulateRateIncrease}
            disabled={isLoading}
            style={styles.button}
          >
            📈 5% Rate Increase
          </button>
          <button
            onClick={handleAddCollateral}
            disabled={isLoading}
            style={styles.button}
          >
            ➕ Add Collateral
          </button>
          <button
            onClick={handleRepayDebt}
            disabled={isLoading}
            style={styles.button}
          >
            💰 Repay Debt
          </button>
        </div>
      </div>

      {simulationResults.length > 0 && (
        <div style={styles.resultsSection}>
          <h3>Simulation Results</h3>
          {simulationResults.map((result, index) => (
            <div key={index} style={styles.resultCard}>
              <div style={styles.resultHeader}>
                <h4>{result.scenarioName}</h4>
                <span
                  style={{
                    ...styles.statusBadge,
                    backgroundColor: result.isLiquidatable ? '#dc3545' : '#28a745',
                  }}
                >
                  {result.isLiquidatable ? 'AT RISK' : 'SAFE'}
                </span>
              </div>
              <div style={styles.resultMetrics}>
                <p>Current Health: {result.currentHealth.toFixed(2)}</p>
                <p>After Scenario: {result.afterScenarioHealth.toFixed(2)}</p>
                <p>Safety Margin: {result.safetyMargin.toFixed(2)}</p>
                <p>Liquidation Price: ${result.liquidationPrice.toFixed(2)}</p>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    padding: '20px',
    maxWidth: '800px',
    margin: '0 auto',
  },
  positionCard: {
    padding: '20px',
    backgroundColor: '#f9f9f9',
    borderRadius: '8px',
    marginBottom: '20px',
    border: '1px solid #e0e0e0',
  },
  positionDetails: {
    display: 'grid',
    gridTemplateColumns: 'repeat(3, 1fr)',
    gap: '15px',
    marginTop: '10px',
  },
  input: {
    width: '100%',
    padding: '8px',
    borderRadius: '4px',
    border: '1px solid #ddd',
    marginTop: '5px',
    boxSizing: 'border-box',
  },
  healthFactor: {
    marginTop: '5px',
    padding: '10px',
    borderRadius: '4px',
    textAlign: 'center',
    color: 'white',
    fontWeight: 'bold',
    fontSize: '18px',
  },
  scenariosSection: {
    marginBottom: '30px',
  },
  buttonsGrid: {
    display: 'grid',
    gridTemplateColumns: 'repeat(2, 1fr)',
    gap: '10px',
    marginTop: '10px',
  },
  button: {
    padding: '12px',
    backgroundColor: '#007bff',
    color: 'white',
    border: 'none',
    borderRadius: '4px',
    cursor: 'pointer',
    fontSize: '14px',
    fontWeight: 'bold',
  },
  resultsSection: {
    marginTop: '20px',
  },
  resultCard: {
    padding: '15px',
    marginBottom: '10px',
    backgroundColor: '#f9f9f9',
    borderRadius: '4px',
    border: '1px solid #e0e0e0',
  },
  resultHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '10px',
  },
  statusBadge: {
    padding: '4px 12px',
    borderRadius: '4px',
    color: 'white',
    fontSize: '12px',
    fontWeight: 'bold',
  },
  resultMetrics: {
    fontSize: '14px',
    lineHeight: '1.6',
  },
};
