import React, { useState } from 'react';

interface MigrationPreview {
  estimatedGas: number;
  estimatedSlippageBps: number;
  interestImpact: number;
  expectedOutput: number;
}

interface MigrationState {
  sourcePool: string;
  destinationPool: string;
  amount: number;
  migrationPercentage: number;
  preview: MigrationPreview | null;
  isLoading: boolean;
}

export const MigrationUI: React.FC = () => {
  const [state, setState] = useState<MigrationState>({
    sourcePool: '',
    destinationPool: '',
    amount: 0,
    migrationPercentage: 100,
    preview: null,
    isLoading: false,
  });

  const handlePreview = async () => {
    setState(prev => ({ ...prev, isLoading: true }));
    try {
      // Call backend API to get migration preview
      const response = await fetch('/api/migration/preview', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sourcePool: state.sourcePool,
          destinationPool: state.destinationPool,
          amount: state.amount,
        }),
      });
      const data: MigrationPreview = await response.json();
      setState(prev => ({ ...prev, preview: data, isLoading: false }));
    } catch (error) {
      console.error('Migration preview failed:', error);
      setState(prev => ({ ...prev, isLoading: false }));
    }
  };

  const handleMigrate = async () => {
    setState(prev => ({ ...prev, isLoading: true }));
    try {
      // Execute migration
      const endpoint = state.migrationPercentage === 100
        ? '/api/migration/full'
        : '/api/migration/partial';

      const response = await fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sourcePool: state.sourcePool,
          destinationPool: state.destinationPool,
          amount: state.amount,
          percentage: state.migrationPercentage,
        }),
      });

      if (response.ok) {
        console.log('Migration completed successfully');
        setState(prev => ({ ...prev, isLoading: false, amount: 0 }));
      }
    } catch (error) {
      console.error('Migration failed:', error);
      setState(prev => ({ ...prev, isLoading: false }));
    }
  };

  return (
    <div style={styles.container}>
      <h2>Pool Migration Tool</h2>

      <div style={styles.formGroup}>
        <label>Source Pool</label>
        <input
          type="text"
          placeholder="Source pool address"
          value={state.sourcePool}
          onChange={e => setState(prev => ({ ...prev, sourcePool: e.target.value }))}
          style={styles.input}
        />
      </div>

      <div style={styles.formGroup}>
        <label>Destination Pool</label>
        <input
          type="text"
          placeholder="Destination pool address"
          value={state.destinationPool}
          onChange={e => setState(prev => ({ ...prev, destinationPool: e.target.value }))}
          style={styles.input}
        />
      </div>

      <div style={styles.formGroup}>
        <label>Amount</label>
        <input
          type="number"
          placeholder="Amount to migrate"
          value={state.amount}
          onChange={e => setState(prev => ({ ...prev, amount: parseFloat(e.target.value) }))}
          style={styles.input}
        />
      </div>

      <div style={styles.formGroup}>
        <label>Migration Percentage: {state.migrationPercentage}%</label>
        <input
          type="range"
          min="1"
          max="100"
          value={state.migrationPercentage}
          onChange={e => setState(prev => ({ ...prev, migrationPercentage: parseInt(e.target.value) }))}
          style={styles.slider}
        />
      </div>

      <button
        onClick={handlePreview}
        disabled={!state.sourcePool || !state.destinationPool || !state.amount}
        style={styles.button}
      >
        {state.isLoading ? 'Loading...' : 'Preview Migration'}
      </button>

      {state.preview && (
        <div style={styles.previewCard}>
          <h3>Migration Preview</h3>
          <p>Estimated Gas: {state.preview.estimatedGas} units</p>
          <p>Estimated Slippage: {(state.preview.estimatedSlippageBps / 100).toFixed(2)}%</p>
          <p>Interest Impact: {state.preview.interestImpact}</p>
          <p>Expected Output: {state.preview.expectedOutput}</p>
          <button onClick={handleMigrate} style={styles.primaryButton}>
            Confirm & Migrate
          </button>
        </div>
      )}
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    padding: '20px',
    border: '1px solid #ddd',
    borderRadius: '8px',
    maxWidth: '600px',
  },
  formGroup: {
    marginBottom: '15px',
  },
  input: {
    width: '100%',
    padding: '8px',
    borderRadius: '4px',
    border: '1px solid #ddd',
    marginTop: '5px',
    boxSizing: 'border-box',
  },
  slider: {
    width: '100%',
    marginTop: '5px',
  },
  button: {
    padding: '10px 20px',
    backgroundColor: '#f0f0f0',
    border: '1px solid #ddd',
    borderRadius: '4px',
    cursor: 'pointer',
    marginRight: '10px',
  },
  primaryButton: {
    padding: '10px 20px',
    backgroundColor: '#007bff',
    color: 'white',
    border: 'none',
    borderRadius: '4px',
    cursor: 'pointer',
    marginTop: '10px',
  },
  previewCard: {
    marginTop: '20px',
    padding: '15px',
    backgroundColor: '#f9f9f9',
    borderRadius: '4px',
    border: '1px solid #e0e0e0',
  },
};
