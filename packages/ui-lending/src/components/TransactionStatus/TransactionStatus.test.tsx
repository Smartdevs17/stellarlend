import React from 'react';
import { render, screen } from '@testing-library/react';
import { TransactionStatus } from './index';

const hash = '3389e9f0f1a65f19736cacf544c2e825313e8447f569233bb8db39aa607c8889';

describe('TransactionStatus', () => {
  it('renders nothing while idle', () => {
    const { container } = render(
      <TransactionStatus tracking={{ hash: null, state: 'idle', startedAt: null }} />
    );
    expect(container.firstChild).toBeNull();
  });

  it('shows a pending transaction as confirming', () => {
    render(<TransactionStatus tracking={{ hash, state: 'pending', startedAt: 0 }} />);
    expect(screen.getByText('Confirming…')).toBeTruthy();
    expect(screen.getByText('3389e9f0…607c8889')).toBeTruthy();
  });

  it('shows the ledger of a confirmed transaction and links to the explorer', () => {
    render(
      <TransactionStatus
        tracking={{ hash, state: 'success', ledger: 1234567, startedAt: 0 }}
        explorerUrl={(txHash) => `https://stellar.expert/explorer/testnet/tx/${txHash}`}
      />
    );
    expect(screen.getByText('Confirmed')).toBeTruthy();
    expect(screen.getByText('Included in ledger 1234567.')).toBeTruthy();
    expect(screen.getByRole('link').getAttribute('href')).toBe(
      `https://stellar.expert/explorer/testnet/tx/${hash}`
    );
  });

  it('shows failures and timeouts', () => {
    const { rerender } = render(
      <TransactionStatus tracking={{ hash, state: 'failed', error: 'The transaction failed on-chain.', startedAt: 0 }} />
    );
    expect(screen.getByText('Failed')).toBeTruthy();
    expect(screen.getByText('The transaction failed on-chain.')).toBeTruthy();

    rerender(<TransactionStatus tracking={{ hash, state: 'timeout', startedAt: 0 }} />);
    expect(screen.getByText('Still pending')).toBeTruthy();
  });
});
