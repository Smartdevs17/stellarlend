import { Request } from 'express';

export interface AuthConfig {
  jwtSecret?: string;
  apiKeyHeader?: string;
  validateApiKey?: (key: string) => Promise<boolean>;
}

export interface JWTPayload {
  userId: string;
  email?: string;
  role?: string;
  [key: string]: any;
}

export interface RateLimitConfig {
  windowMs: number;
  maxCallsPerWindow: number;
  burstCalls: number;
  graceBurstCalls: number;
}

export interface CongestionConfig {
  enabled: boolean;
  baselineSecsPerLedger: number;
  reportTtlSeconds: number;
  minFactorBps: number;
  maxFactorBps: number;
}

export interface RateLimitPolicy {
  /** Default rate limit per operation type */
  defaultLimits: Record<string, RateLimitConfig>;
  /** Congestion adaptation config */
  congestion: CongestionConfig;
  /** Whether congestion adaptation is enabled */
  enabled: boolean;
}

/** Default rate limit policy for the protocol */
export const defaultRateLimitPolicy: RateLimitPolicy = {
  defaultLimits: {
    borrow: {
      windowMs: 60_000,
      maxCallsPerWindow: 5,
      burstCalls: 3,
      graceBurstCalls: 10,
    },
    liquidate: {
      windowMs: 60_000,
      maxCallsPerWindow: 10,
      burstCalls: 5,
      graceBurstCalls: 20,
    },
    deposit: {
      windowMs: 60_000,
      maxCallsPerWindow: 30,
      burstCalls: 10,
      graceBurstCalls: 0,
    },
    repay: {
      windowMs: 60_000,
      maxCallsPerWindow: 30,
      burstCalls: 10,
      graceBurstCalls: 0,
    },
  },
  congestion: {
    enabled: false,
    baselineSecsPerLedger: 5,
    reportTtlSeconds: 300,
    minFactorBps: 2_500,
    maxFactorBps: 10_000,
  },
  enabled: false,
};
