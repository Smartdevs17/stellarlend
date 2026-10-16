import React from 'react';
import { useOnlineStatus } from '../pwa/serviceWorker';

/**
 * Shown while the browser is offline. The service worker keeps the app shell
 * and the last loaded data readable; transactions need a connection.
 */
export const OfflineStatusBanner: React.FC = () => {
  const online = useOnlineStatus();
  if (online) return null;

  return (
    <div
      role="status"
      style={{
        padding: '8px 16px',
        background: '#fff4e5',
        color: '#8a4b00',
        borderBottom: '1px solid #f5c26b',
        fontFamily: 'sans-serif',
        fontSize: 14,
      }}
    >
      You are offline. Showing the last loaded data — transactions will be available again once you
      reconnect.
    </div>
  );
};
