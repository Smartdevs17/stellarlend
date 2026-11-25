export {
  ContractDomain,
  PROTOCOL_ERROR_CATALOG,
  ProtocolErrorCode,
  getProtocolErrorMetadata,
  isProtocolErrorCode,
} from './codes.ts';
export type { ProtocolErrorMetadata } from './codes.ts';

export {
  BACKEND_ERROR_MAPPINGS,
  CONTRACT_ERROR_MAPPINGS,
  UnifiedProtocolError,
  mapBackendErrorCode,
  mapContractError,
  normalizeError,
} from './mappings.ts';

export {
  emitErrorTelemetry,
  getErrorObserver,
  setErrorObserver,
} from './observability.ts';
export type { ErrorObserver, ErrorTelemetryEvent } from './observability.ts';
