import { renderHook, waitFor } from '@testing-library/react';
import { horizonTransactionStatus, useTransactionStatus } from './useTransactionStatus';

const hash = '3389e9f0f1a65f19736cacf544c2e825313e8447f569233bb8db39aa607c8889';

describe('useTransactionStatus', () => {
  it('is idle without a hash', () => {
    const fetchStatus = jest.fn();
    const { result } = renderHook(() => useTransactionStatus(null, { fetchStatus }));

    expect(result.current.state).toBe('idle');
    expect(fetchStatus).not.toHaveBeenCalled();
  });

  it('polls until the transaction is confirmed', async () => {
    const fetchStatus = jest
      .fn()
      .mockResolvedValueOnce({ status: 'pending' })
      .mockRejectedValueOnce(new Error('network blip'))
      .mockResolvedValueOnce({ status: 'success', ledger: 42 });

    const { result } = renderHook(() => useTransactionStatus(hash, { fetchStatus, intervalMs: 10 }));

    expect(result.current.state).toBe('pending');
    await waitFor(() => expect(result.current.state).toBe('success'));
    expect(result.current.ledger).toBe(42);
    expect(fetchStatus).toHaveBeenCalledTimes(3);
    expect(fetchStatus).toHaveBeenCalledWith(hash);
  });

  it('reports failures and timeouts', async () => {
    const failed = renderHook(() =>
      useTransactionStatus(hash, {
        fetchStatus: jest.fn().mockResolvedValue({ status: 'failed', error: 'The transaction failed on-chain.' }),
      })
    );
    await waitFor(() => expect(failed.result.current.state).toBe('failed'));
    expect(failed.result.current.error).toBe('The transaction failed on-chain.');

    const slow = renderHook(() =>
      useTransactionStatus(hash, {
        fetchStatus: jest.fn().mockResolvedValue({ status: 'pending' }),
        intervalMs: 10,
        timeoutMs: 0,
      })
    );
    await waitFor(() => expect(slow.result.current.state).toBe('timeout'));
  });
});

describe('horizonTransactionStatus', () => {
  const originalFetch = global.fetch;

  afterEach(() => {
    global.fetch = originalFetch;
  });

  function mockFetch(status: number, body?: unknown) {
    global.fetch = jest.fn().mockResolvedValue({
      status,
      ok: status >= 200 && status < 300,
      json: async () => body,
    }) as unknown as typeof fetch;
  }

  it('treats 404 as still pending', async () => {
    mockFetch(404);
    await expect(horizonTransactionStatus('https://horizon-testnet.stellar.org/')(hash)).resolves.toEqual({
      status: 'pending',
    });
    expect(global.fetch).toHaveBeenCalledWith(`https://horizon-testnet.stellar.org/transactions/${hash}`);
  });

  it('maps the Horizon result to success or failure', async () => {
    mockFetch(200, { successful: true, ledger: 99 });
    await expect(horizonTransactionStatus('https://horizon.example')(hash)).resolves.toEqual({
      status: 'success',
      ledger: 99,
    });

    mockFetch(200, { successful: false, ledger: 100 });
    await expect(horizonTransactionStatus('https://horizon.example')(hash)).resolves.toMatchObject({
      status: 'failed',
      ledger: 100,
    });
  });
});
