import React, { useState } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { ErrorBoundary } from './index';

function Broken(): React.ReactElement {
  throw new Error('TypeError: Failed to fetch');
}

describe('ErrorBoundary', () => {
  let consoleError: jest.SpyInstance;

  beforeEach(() => {
    // React logs caught render errors; keep the test output clean.
    consoleError = jest.spyOn(console, 'error').mockImplementation(() => undefined);
  });

  afterEach(() => {
    consoleError.mockRestore();
  });

  it('renders children when nothing throws', () => {
    render(
      <ErrorBoundary>
        <span>Healthy content</span>
      </ErrorBoundary>
    );
    expect(screen.getByText('Healthy content')).toBeTruthy();
  });

  it('shows a friendly message instead of the raw error and reports it', () => {
    const onError = jest.fn();
    render(
      <ErrorBoundary onError={onError}>
        <Broken />
      </ErrorBoundary>
    );

    expect(screen.getByRole('alert')).toBeTruthy();
    expect(screen.getByText(/check your connection/i)).toBeTruthy();
    expect(screen.queryByText(/TypeError/)).toBeNull();
    expect(onError).toHaveBeenCalledTimes(1);
  });

  it('renders the children again after "Try again"', () => {
    function Flaky() {
      const [broken, setBroken] = useState(true);
      return (
        <ErrorBoundary onReset={() => setBroken(false)}>
          {broken ? <Broken /> : <span>Recovered</span>}
        </ErrorBoundary>
      );
    }

    render(<Flaky />);
    fireEvent.click(screen.getByText('Try again'));
    expect(screen.getByText('Recovered')).toBeTruthy();
  });

  it('supports a custom fallback', () => {
    render(
      <ErrorBoundary fallback={({ message }) => <p>Custom: {message}</p>}>
        <Broken />
      </ErrorBoundary>
    );
    expect(screen.getByText(/Custom: .*check your connection/i)).toBeTruthy();
  });
});
