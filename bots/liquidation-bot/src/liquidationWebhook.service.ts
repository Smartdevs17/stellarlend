import { createHmac } from 'crypto';
import axios, { AxiosInstance } from 'axios';

import { Logger } from './logger';

export interface LiquidationWebhookConfig {
  url: string;
  secret: string;
  maxAttempts: number;
  retryDelayMs: number;
}

export interface ConfirmedLiquidation {
  positionAddress: string;
  txHash: string;
  profitStroops: number;
  timestamp: number;
}

export interface LiquidationWebhookPayload {
  id: string;
  type: 'liquidation.confirmed';
  createdAt: number;
  data: ConfirmedLiquidation;
}

export function signWebhookPayload(secret: string, timestamp: number, body: string): string {
  return `sha256=${createHmac('sha256', secret).update(`${timestamp}.${body}`).digest('hex')}`;
}

export class LiquidationWebhookService {
  constructor(
    private readonly config: LiquidationWebhookConfig,
    private readonly logger: Logger,
    private readonly http: AxiosInstance = axios,
    private readonly sleep: (delayMs: number) => Promise<void> = (delayMs) =>
      new Promise((resolve) => setTimeout(resolve, delayMs)),
  ) {}

  public isConfigured(): boolean {
    return Boolean(this.config.url && this.config.secret);
  }

  public async deliver(liquidation: ConfirmedLiquidation): Promise<boolean> {
    if (!this.config.url && !this.config.secret) return false;
    if (!this.isConfigured()) {
      this.logger.warn('Liquidation webhook is disabled because URL and secret are both required');
      return false;
    }

    const payload: LiquidationWebhookPayload = {
      id: `liquidation_${liquidation.txHash || liquidation.positionAddress}_${liquidation.timestamp}`,
      type: 'liquidation.confirmed',
      createdAt: liquidation.timestamp,
      data: liquidation,
    };
    const body = JSON.stringify(payload);
    const timestamp = Math.floor(Date.now() / 1000);
    const headers = {
      'content-type': 'application/json',
      'x-stellarlend-webhook-id': payload.id,
      'x-stellarlend-webhook-timestamp': String(timestamp),
      'x-stellarlend-webhook-signature': signWebhookPayload(this.config.secret, timestamp, body),
    };
    const maxAttempts = Math.max(1, this.config.maxAttempts);

    for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
      try {
        await this.http.post(this.config.url, body, { headers, timeout: 10000 });
        this.logger.info('Liquidation webhook delivered', { id: payload.id, attempt });
        return true;
      } catch (error: unknown) {
        const status = axios.isAxiosError(error) ? error.response?.status : undefined;
        const retryable = status === undefined || status === 429 || status >= 500;
        if (!retryable || attempt === maxAttempts) {
          this.logger.error('Liquidation webhook delivery failed', { id: payload.id, attempt, status });
          return false;
        }
        this.logger.warn('Liquidation webhook delivery failed; retrying', { id: payload.id, attempt, status });
        await this.sleep(this.config.retryDelayMs * 2 ** (attempt - 1));
      }
    }
    return false;
  }
}
