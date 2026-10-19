import { useEffect, useRef, useState } from 'react';
import type { TransactionStatusResult, TransactionTracking } from '../types';

export type FetchTransactionStatus = (hash: string) => Promise<TransactionStatusResult>;

interface UseTransactionStatusOptions {
  /** Looks the transaction up, e.g. {@link horizonTransactionStatus}. */
  fetchStatus: FetchTransactionStatus;
  /** Poll interval in ms (default 3000). */
  intervalMs?: number;
  /** Stop waiting after this many ms without a final result (default 120000). */
  timeoutMs?: number;
}

/**
 * Status lookup backed by Horizon's `GET /transactions/:hash`, which answers
 * 404 until the transaction has been included in a ledger.
 */
export function horizonTransactionStatus(horizonUrl: string): FetchTransactionStatus {
  const base = horizonUrl.replace(/\/$/, '');
  return async (hash) => {
    const res = await fetch(`${base}/transactions/${hash}`);
    if (res.status === 404) return { status: 'pending' };
    if (!res.ok) throw new Error(`Horizon responded ${res.status}`);

    const tx = (await res.json()) as { successful?: boolean; ledger?: number };
    return tx.successful
      ? { status: 'success', ledger: tx.ledger }
      : { status: 'failed', ledger: tx.ledger, error: 'The transaction failed on-chain.' };
  };
}

/**
 * useTransactionStatus — tracks a submitted transaction by hash until it is
 * confirmed, fails, or the timeout passes. A failed lookup (e.g. a network
 * blip) is retried on the next poll rather than reported as a failure.
 */
export function useTransactionStatus(
  hash: string | null | undefined,
  options: UseTransactionStatusOptions
): TransactionTracking {
  const { intervalMs = 3000, timeoutMs = 120000 } = options;
  const fetchStatus = useRef(options.fetchStatus);
  fetchStatus.current = options.fetchStatus;

  const [tracking, setTracking] = useState<TransactionTracking>({
    hash: null,
    state: 'idle',
    startedAt: null,
  });

  useEffect(() => {
    if (!hash) {
      setTracking({ hash: null, state: 'idle', startedAt: null });
      return undefined;
    }

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const startedAt = Date.now();
    setTracking({ hash, state: 'pending', startedAt });

    const poll = async () => {
      try {
        const result = await fetchStatus.current(hash);
        if (cancelled) return;
        if (result.status !== 'pending') {
          setTracking({ hash, state: result.status, ledger: result.ledger, error: result.error, startedAt });
          return;
        }
      } catch {
        if (cancelled) return;
      }

      if (Date.now() - startedAt >= timeoutMs) {
        setTracking({ hash, state: 'timeout', startedAt });
        return;
      }
      timer = setTimeout(poll, intervalMs);
    };

    void poll();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [hash, intervalMs, timeoutMs]);

  return tracking;
}
