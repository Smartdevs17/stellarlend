import React, { useEffect, useState } from 'react';
import type { LendingTheme, Position } from '../../types';
import { LIGHT_COLORS } from '../../utils/theme';
import { responsiveGrid, TOUCH_TARGET_SIZE } from '../../utils/responsive';
import { useHealthFactor } from '../../hooks/useHealthFactor';
import { HealthMeter } from '../HealthMeter';
import { PositionCard } from '../PositionCard';

interface PositionMonitorProps {
  positions: Position[];
  /** When the positions were last refreshed (ms since epoch). */
  lastUpdated?: number | null;
  /** Data older than this is flagged as delayed (default 30000 ms). */
  staleAfterMs?: number;
  isLoading?: boolean;
  error?: string | null;
  onRefresh?: () => void;
  onSupply?: (position: Position) => void;
  onBorrow?: (position: Position) => void;
  onRepay?: (position: Position) => void;
  onWithdraw?: (position: Position) => void;
  theme?: LendingTheme;
  className?: string;
}

const HEALTH_WARNINGS: Partial<Record<string, string>> = {
  warning: 'Your health factor is getting low. Consider adding collateral.',
  danger: 'Your position is close to liquidation. Add collateral or repay debt now.',
  liquidatable: 'Your position can be liquidated. Repay debt or add collateral immediately.',
};

function formatAge(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  return seconds < 60 ? `${seconds}s ago` : `${Math.round(seconds / 60)}m ago`;
}

/**
 * PositionMonitor — live dashboard of a user's lending positions: totals,
 * account health, a warning when liquidation risk rises, and each position
 * with its actions. Pair it with {@link useLivePositions} for real-time data.
 */
export function PositionMonitor({
  positions,
  lastUpdated = null,
  staleAfterMs = 30000,
  isLoading = false,
  error = null,
  onRefresh,
  onSupply,
  onBorrow,
  onRepay,
  onWithdraw,
  theme,
  className = '',
}: PositionMonitorProps) {
  const colors = theme?.colors ?? LIGHT_COLORS;
  const { healthFactor } = useHealthFactor(positions);
  const [now, setNow] = useState(() => Date.now());

  // Keep the "updated … ago" label and the delayed flag current.
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  const totalSupplied = positions.reduce((sum, p) => sum + p.supplied * p.price, 0);
  const totalBorrowed = positions.reduce((sum, p) => sum + p.borrowed * p.price, 0);
  const isStale = lastUpdated !== null && now - lastUpdated > staleAfterMs;
  const healthWarning = positions.length > 0 ? HEALTH_WARNINGS[healthFactor.status] : undefined;

  const summary = [
    { label: 'Total supplied', value: totalSupplied, color: colors.success },
    { label: 'Total borrowed', value: totalBorrowed, color: colors.danger },
    { label: 'Net value', value: totalSupplied - totalBorrowed, color: colors.text },
  ];

  return (
    <div className={`position-monitor ${className}`} style={{ display: 'flex', flexDirection: 'column', gap: 16 }} data-testid="position-monitor">
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 8 }}>
        <h2 style={{ color: colors.text, margin: 0, fontSize: 20, fontWeight: 600 }}>Your positions</h2>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <span style={{ color: isStale ? colors.warning : colors.textMuted, fontSize: 13 }} aria-live="polite">
            <span aria-hidden="true" style={{ color: isStale ? colors.warning : colors.success }}>● </span>
            {lastUpdated === null ? 'Connecting…' : `${isStale ? 'Delayed' : 'Live'} · updated ${formatAge(now - lastUpdated)}`}
          </span>
          {onRefresh && (
            <button
              onClick={onRefresh}
              style={{ minHeight: TOUCH_TARGET_SIZE, padding: '8px 12px', background: colors.surface, color: colors.text, border: `1px solid ${colors.border}`, borderRadius: 8, cursor: 'pointer', fontSize: 13 }}
            >
              Refresh
            </button>
          )}
        </div>
      </div>

      {error && (
        <div role="alert" style={{ color: colors.danger, fontSize: 13 }}>
          {error}
        </div>
      )}

      {healthWarning && (
        <div
          role="alert"
          style={{ background: colors.surface, border: `1px solid ${colors.danger}`, borderRadius: 12, padding: 12, color: colors.danger, fontSize: 14, fontWeight: 500 }}
        >
          {healthWarning}
        </div>
      )}

      <div style={responsiveGrid(160)}>
        {summary.map((item) => (
          <div key={item.label} style={{ background: colors.surface, border: `1px solid ${colors.border}`, borderRadius: 12, padding: 16 }}>
            <div style={{ color: colors.textMuted, fontSize: 12, marginBottom: 4 }}>{item.label}</div>
            <div style={{ color: item.color, fontSize: 18, fontWeight: 600 }}>${item.value.toFixed(2)}</div>
          </div>
        ))}
      </div>

      <HealthMeter health={healthFactor} theme={theme} isLoading={isLoading && positions.length === 0} />

      {isLoading && positions.length === 0 ? (
        <div style={responsiveGrid(280, 16)}>
          <PositionCard position={{ id: 'loading', asset: '', symbol: '', supplied: 0, borrowed: 0, supplyApy: 0, borrowApy: 0, collateralFactor: 0, price: 0 }} theme={theme} isLoading />
        </div>
      ) : positions.length === 0 ? (
        <p style={{ color: colors.textMuted, fontSize: 14, margin: 0 }}>You have no open positions yet.</p>
      ) : (
        <div style={responsiveGrid(280, 16)}>
          {positions.map((position) => (
            <PositionCard
              key={position.id}
              position={position}
              theme={theme}
              onSupply={onSupply}
              onBorrow={onBorrow}
              onRepay={onRepay}
              onWithdraw={onWithdraw}
            />
          ))}
        </div>
      )}
    </div>
  );
}
