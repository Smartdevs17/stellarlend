import { ContractDomain, ProtocolErrorCode } from './codes.ts';

export interface ErrorTelemetryEvent {
  code: ProtocolErrorCode;
  slug: string;
  httpStatus: number;
  retryable: boolean;
  domain?: ContractDomain;
  localCode?: number;
  backendCode?: string;
  traceId?: string;
  tags?: Record<string, string | number | boolean>;
}

export interface ErrorObserver {
  onErrorNormalized(event: ErrorTelemetryEvent): void;
}

let activeObserver: ErrorObserver | null = null;

/**
 * Registers an observability hook for error normalization (logs, metrics, traces).
 * Pass `null` to clear the observer.
 */
export function setErrorObserver(observer: ErrorObserver | null): void {
  activeObserver = observer;
}

export function getErrorObserver(): ErrorObserver | null {
  return activeObserver;
}

export function emitErrorTelemetry(event: ErrorTelemetryEvent): void {
  if (!activeObserver) {
    return;
  }
  try {
    activeObserver.onErrorNormalized(event);
  } catch {
    // Observability hooks must never crash error handling paths.
  }
}
