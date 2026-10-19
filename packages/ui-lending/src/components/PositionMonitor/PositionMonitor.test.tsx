import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { PositionMonitor } from './index';

const usdc = {
  id: 'usdc',
  asset: 'USD Coin',
  symbol: 'USDC',
  supplied: 1000,
  borrowed: 0,
  supplyApy: 4.5,
  borrowApy: 6.2,
  collateralFactor: 0.8,
  price: 1,
};

describe('PositionMonitor', () => {
  it('shows totals, account health and each position', () => {
    render(<PositionMonitor positions={[usdc, { ...usdc, id: 'usdc-2', supplied: 0, borrowed: 250 }]} lastUpdated={Date.now()} />);

    expect(screen.getByText('Total supplied').nextSibling?.textContent).toBe('$1000.00');
    expect(screen.getByText('Total borrowed').nextSibling?.textContent).toBe('$250.00');
    expect(screen.getByText('Net value').nextSibling?.textContent).toBe('$750.00');
    expect(screen.getByTestId('health-meter')).toBeTruthy();
    expect(screen.getAllByTestId('position-card')).toHaveLength(2);
    expect(screen.getByText(/Live · updated/)).toBeTruthy();
  });

  it('warns when the position is close to liquidation', () => {
    // 1000 USDC × 0.8 collateral factor × 0.8 threshold / 600 borrowed ≈ 1.07
    render(<PositionMonitor positions={[{ ...usdc, borrowed: 600 }]} lastUpdated={Date.now()} />);
    expect(screen.getByText(/close to liquidation/i)).toBeTruthy();
  });

  it('flags delayed data and shows errors', () => {
    render(
      <PositionMonitor
        positions={[usdc]}
        lastUpdated={Date.now() - 120000}
        error="We couldn't reach StellarLend. Check your connection and try again."
      />
    );
    expect(screen.getByText(/Delayed · updated 2m ago/)).toBeTruthy();
    expect(screen.getByText(/couldn't reach StellarLend/)).toBeTruthy();
  });

  it('shows an empty state and triggers refresh', () => {
    const onRefresh = jest.fn();
    render(<PositionMonitor positions={[]} lastUpdated={Date.now()} onRefresh={onRefresh} />);

    expect(screen.getByText('You have no open positions yet.')).toBeTruthy();
    fireEvent.click(screen.getByText('Refresh'));
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });
});
