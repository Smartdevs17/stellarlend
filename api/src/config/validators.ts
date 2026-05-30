import { AppConfig } from './types';
import { ValidationError } from '../utils/errors';

export function validateConfig(config: AppConfig): string[] {
  const errors: string[] = [];

  if (!config.stellar.contractId) {
    errors.push('CONTRACT_ID is required');
  }

  if (!config.auth.jwtSecret || config.auth.jwtSecret.length < 32) {
    errors.push('JWT_SECRET must be at least 32 characters');
  }

  if (config.server.port < 1 || config.server.port > 65535) {
    errors.push('PORT must be between 1 and 65535');
  }

  if (config.cache.idempotencyTtlMs < 1000) {
    errors.push('IDEMPOTENCY_TTL_MS must be at least 1000');
  }

  if (config.pagination.maxLimit < config.pagination.defaultLimit) {
    errors.push('PAGINATION_MAX_LIMIT must be >= PAGINATION_DEFAULT_LIMIT');
  }

  if (config.analytics.historyRetentionDays < 1) {
    errors.push('ANALYTICS_HISTORY_RETENTION_DAYS must be >= 1');
  }

  if (config.subscriptions.maxRetries < 0) {
    errors.push('SUBSCRIPTION_MAX_RETRIES must be >= 0');
  }

  if (config.ws.priceUpdateIntervalMs < 1000) {
    errors.push('WS_PRICE_UPDATE_INTERVAL_MS must be >= 1000');
  }

  if (config.emergency.autoPauseFailureThreshold < 1) {
    errors.push('AUTO_PAUSE_FAILURE_THRESHOLD must be >= 1');
  }

  return errors;
}

export function assertValidConfig(config: AppConfig): void {
  const errors = validateConfig(config);
  if (errors.length > 0) {
    throw new ValidationError(`Config validation failed: ${errors.join('; ')}`);
  }
}
