import React, { useState } from 'react';
import { isPushSupported, subscribeToLiquidationWarnings } from '../pwa/pushNotifications';

interface LiquidationWarningPushToggleProps {
  /** Connected wallet address the warnings are sent for. */
  userAddress: string;
}

/** Lets the user opt in to push notifications for liquidation warnings. */
export const LiquidationWarningPushToggle: React.FC<LiquidationWarningPushToggleProps> = ({
  userAddress,
}) => {
  const [status, setStatus] = useState<'idle' | 'subscribing' | 'enabled'>('idle');
  const [error, setError] = useState<string | null>(null);

  if (!isPushSupported()) {
    return <p style={{ fontFamily: 'sans-serif' }}>Push notifications are not supported in this browser.</p>;
  }

  const enable = async () => {
    setError(null);
    setStatus('subscribing');
    try {
      await subscribeToLiquidationWarnings(userAddress);
      setStatus('enabled');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to enable push notifications');
      setStatus('idle');
    }
  };

  return (
    <div style={{ fontFamily: 'sans-serif' }}>
      <button onClick={enable} disabled={status !== 'idle'}>
        {status === 'enabled'
          ? 'Liquidation warnings enabled'
          : status === 'subscribing'
            ? 'Enabling…'
            : 'Notify me before liquidation'}
      </button>
      {error && <p style={{ color: '#c00' }}>{error}</p>}
    </div>
  );
};
