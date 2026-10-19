import React from 'react';
import type { ErrorInfo, ReactNode } from 'react';
import type { LendingTheme } from '../../types';
import { LIGHT_COLORS } from '../../utils/theme';
import { getFriendlyErrorMessage } from '../../utils/errorMessages';
import { actionButtonBase } from '../../utils/responsive';

interface ErrorBoundaryFallbackProps {
  error: Error;
  /** User-friendly description of the error. */
  message: string;
  /** Clears the error and renders the children again. */
  reset: () => void;
}

interface ErrorBoundaryProps {
  children: ReactNode;
  /** Custom fallback UI. Defaults to a themed card with a "Try again" button. */
  fallback?: (props: ErrorBoundaryFallbackProps) => ReactNode;
  /** Called with the caught error, e.g. to report it to an error tracker. */
  onError?: (error: Error, info: ErrorInfo) => void;
  /** Called when the user retries after an error. */
  onReset?: () => void;
  theme?: LendingTheme;
  className?: string;
}

interface ErrorBoundaryState {
  error: Error | null;
}

/**
 * ErrorBoundary — catches rendering errors in the lending UI and shows a
 * user-friendly message instead of a blank screen or a stack trace.
 */
export class ErrorBoundary extends React.Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    this.props.onError?.(error, info);
  }

  reset = (): void => {
    this.props.onReset?.();
    this.setState({ error: null });
  };

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;

    const message = getFriendlyErrorMessage(error);
    if (this.props.fallback) {
      return this.props.fallback({ error, message, reset: this.reset });
    }

    const colors = this.props.theme?.colors ?? LIGHT_COLORS;
    return (
      <div
        className={`error-boundary ${this.props.className ?? ''}`}
        style={{ background: colors.surface, border: `1px solid ${colors.border}`, borderRadius: 12, padding: 20 }}
        role="alert"
        data-testid="error-boundary"
      >
        <h3 style={{ color: colors.text, margin: '0 0 8px', fontSize: 18, fontWeight: 600 }}>Something went wrong</h3>
        <p style={{ color: colors.textMuted, margin: '0 0 16px', fontSize: 14 }}>{message}</p>
        <button
          onClick={this.reset}
          style={{ ...actionButtonBase, flex: 'none', background: colors.primary, color: '#fff', border: 'none' }}
        >
          Try again
        </button>
      </div>
    );
  }
}
