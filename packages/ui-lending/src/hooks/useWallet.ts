import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';

export interface WalletSnapshot {
  status: 'disconnected' | 'connecting' | 'connected' | 'disconnecting';
  providerId: string | null;
  address: string | null;
  network: string | null;
  error: Error | null;
}

export interface WalletChoice {
  id: string;
  name: string;
  icon?: string;
  available: boolean;
}

export interface WalletConnectionManager {
  getState(): WalletSnapshot;
  subscribe(listener: () => void): () => void;
  getAvailableProviders(): Promise<WalletChoice[]>;
  connect(providerId: string): Promise<{ address: string; network?: string }>;
  disconnect(): Promise<void>;
  signTransaction(xdr: string, options?: { networkPassphrase?: string; accountToSign?: string }): Promise<string>;
}

const serverSnapshot: WalletSnapshot = {
  status: 'disconnected',
  providerId: null,
  address: null,
  network: null,
  error: null,
};

/** React binding for a multi-provider WalletManager-compatible object. */
export function useWallet(manager: WalletConnectionManager) {
  const snapshot = useSyncExternalStore(manager.subscribe, manager.getState, () => serverSnapshot);
  const [providers, setProviders] = useState<WalletChoice[]>([]);
  const [providerError, setProviderError] = useState<Error | null>(null);

  const refreshProviders = useCallback(async () => {
    try {
      const available = await manager.getAvailableProviders();
      setProviders(available);
      setProviderError(null);
      return available;
    } catch (value) {
      const error = value instanceof Error ? value : new Error('Could not detect wallet providers');
      setProviderError(error);
      throw error;
    }
  }, [manager]);

  useEffect(() => {
    void refreshProviders().catch(() => undefined);
  }, [refreshProviders]);

  const connect = useCallback((providerId: string) => manager.connect(providerId), [manager]);
  const disconnect = useCallback(() => manager.disconnect(), [manager]);
  const signTransaction = useCallback(
    (xdr: string, options?: { networkPassphrase?: string; accountToSign?: string }) =>
      manager.signTransaction(xdr, options),
    [manager]
  );

  return {
    ...snapshot,
    error: snapshot.error ?? providerError,
    providers,
    isConnected: snapshot.status === 'connected',
    isConnecting: snapshot.status === 'connecting',
    connect,
    disconnect,
    signTransaction,
    refreshProviders,
  };
}
