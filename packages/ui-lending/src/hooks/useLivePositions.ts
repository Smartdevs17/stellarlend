import { useCallback, useEffect, useRef, useState } from 'react';
import type { Position } from '../types';
import { getFriendlyErrorMessage } from '../utils/errorMessages';

interface UseLivePositionsOptions {
  /** Loads the user's current positions, e.g. from the StellarLend API. */
  fetchPositions: () => Promise<Position[]>;
  /** Refresh interval in ms (default 10000). */
  intervalMs?: number;
  /**
   * Optional push source such as a WebSocket: called with a callback that
   * receives updated positions, and returns an unsubscribe function.
   */
  subscribe?: (onUpdate: (positions: Position[]) => void) => () => void;
}

/**
 * useLivePositions — keeps a user's positions up to date by polling and,
 * when a `subscribe` source is given, by applying pushed updates as they
 * arrive.
 */
export function useLivePositions({ fetchPositions, intervalMs = 10000, subscribe }: UseLivePositionsOptions) {
  const [positions, setPositions] = useState<Position[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<number | null>(null);
  const fetchRef = useRef(fetchPositions);
  fetchRef.current = fetchPositions;

  const applyUpdate = useCallback((next: Position[]) => {
    setPositions(next);
    setLastUpdated(Date.now());
    setError(null);
  }, []);

  const refresh = useCallback(async () => {
    try {
      applyUpdate(await fetchRef.current());
    } catch (err) {
      setError(getFriendlyErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }, [applyUpdate]);

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), intervalMs);
    return () => clearInterval(timer);
  }, [refresh, intervalMs]);

  useEffect(() => {
    if (!subscribe) return undefined;
    return subscribe(applyUpdate);
  }, [subscribe, applyUpdate]);

  return { positions, isLoading, error, lastUpdated, refresh };
}
