import { DEFAULT_ERROR_MESSAGE, getFriendlyErrorMessage } from './errorMessages';

describe('getFriendlyErrorMessage', () => {
  it('explains network failures', () => {
    expect(getFriendlyErrorMessage(new TypeError('Failed to fetch'))).toMatch(/check your connection/i);
  });

  it('explains a request cancelled in the wallet', () => {
    expect(getFriendlyErrorMessage(new Error('User declined access'))).toBe(
      'The request was cancelled in your wallet.'
    );
  });

  it('explains timeouts, rate limits and low balances', () => {
    expect(getFriendlyErrorMessage('Request timed out')).toMatch(/too long/i);
    expect(getFriendlyErrorMessage(new Error('HTTP 429 Too Many Requests'))).toMatch(/too many requests/i);
    expect(getFriendlyErrorMessage(new Error('op_underfunded'))).toMatch(/balance is too low/i);
  });

  it('explains liquidation risk and paused lending', () => {
    expect(getFriendlyErrorMessage(new Error('Health factor below 1.0'))).toMatch(/risk of liquidation/i);
    expect(getFriendlyErrorMessage(new Error('Protocol is paused'))).toMatch(/temporarily paused/i);
  });

  it('hides unknown technical errors behind a generic message', () => {
    expect(getFriendlyErrorMessage(new Error("Cannot read properties of undefined (reading 'value')"))).toBe(
      DEFAULT_ERROR_MESSAGE
    );
    expect(getFriendlyErrorMessage(undefined)).toBe(DEFAULT_ERROR_MESSAGE);
  });
});
