import React from 'react';
import type { LendingTheme, ThemeColors, TransactionTracking } from '../../types';
import { LIGHT_COLORS } from '../../utils/theme';

interface TransactionStatusProps {
  /** Result of {@link useTransactionStatus}. Nothing is rendered while idle. */
  tracking: TransactionTracking;
  /** Builds a block-explorer link for the hash, e.g. to stellar.expert. */
  explorerUrl?: (hash: string) => string;
  theme?: LendingTheme;
  className?: string;
}

function describe(tracking: TransactionTracking, colors: ThemeColors) {
  switch (tracking.state) {
    case 'success':
      return {
        label: 'Confirmed',
        color: colors.success,
        message: tracking.ledger ? `Included in ledger ${tracking.ledger}.` : 'Your transaction is confirmed.',
      };
    case 'failed':
      return {
        label: 'Failed',
        color: colors.danger,
        message: tracking.error ?? 'The transaction failed.',
      };
    case 'timeout':
      return {
        label: 'Still pending',
        color: colors.warning,
        message: 'Confirmation is taking longer than expected. Check the explorer for the latest status.',
      };
    default:
      return {
        label: 'Confirming…',
        color: colors.primary,
        message: 'Your transaction was submitted and is waiting to be included in a ledger.',
      };
  }
}

function shortHash(hash: string): string {
  return hash.length > 16 ? `${hash.slice(0, 8)}…${hash.slice(-8)}` : hash;
}

/**
 * TransactionStatus — shows where a submitted transaction is: confirming,
 * confirmed, failed or still pending after the timeout.
 */
export function TransactionStatus({ tracking, explorerUrl, theme, className = '' }: TransactionStatusProps) {
  const colors = theme?.colors ?? LIGHT_COLORS;
  if (tracking.state === 'idle' || !tracking.hash) return null;

  const { label, color, message } = describe(tracking, colors);

  return (
    <div
      className={`transaction-status transaction-status--${tracking.state} ${className}`}
      style={{ background: colors.surface, border: `1px solid ${colors.border}`, borderRadius: 12, padding: 16 }}
      role="status"
      aria-live="polite"
      data-testid="transaction-status"
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 8, marginBottom: 8 }}>
        <span style={{ color, fontWeight: 600 }}>{label}</span>
        {explorerUrl ? (
          <a href={explorerUrl(tracking.hash)} target="_blank" rel="noopener noreferrer" style={{ color: colors.primary, fontSize: 13 }}>
            {shortHash(tracking.hash)}
          </a>
        ) : (
          <span style={{ color: colors.textMuted, fontSize: 13 }}>{shortHash(tracking.hash)}</span>
        )}
      </div>
      <p style={{ color: colors.textMuted, margin: 0, fontSize: 13 }}>{message}</p>
    </div>
  );
}
