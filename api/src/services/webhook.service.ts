import { createHmac, randomBytes, timingSafeEqual } from 'crypto';
import logger from '../utils/logger';

/**
 * Issue #1376 – Webhook notifications for liquidations.
 *
 * Deliveries are HMAC-SHA256 signed so receivers can verify StellarLend as
 * the sender, retried with capped exponential backoff, and documented in
 * `docs/WEBHOOKS.md`.
 */

export type WebhookEvent =
  | 'liquidation.alert'
  | 'liquidation.created'
  | 'webhook.test'
  | (string & {});

export interface WebhookSubscription {
  id: string;
  url: string;
  /** Signing secret – returned once at creation, never listed. */
  secret: string;
  events: string[];
  createdAt: string;
  active: boolean;
}

export interface WebhookDelivery {
  id: string;
  event: WebhookEvent;
  url: string;
  status: 'delivered' | 'failed';
  attempts: number;
  statusCode?: number;
  error?: string;
  durationMs: number;
}

export interface WebhookPayload {
  id: string;
  type: WebhookEvent;
  createdAt: string;
  data: unknown;
}

export type WebhookFetcher = (
  url: string,
  init: { method: string; headers: Record<string, string>; body: string }
) => Promise<{ ok: boolean; status: number }>;

const MAX_ATTEMPTS = 3;
const BASE_DELAY_MS = Math.max(1, Number(process.env.WEBHOOK_RETRY_BASE_MS ?? 100));
const MAX_DELAY_MS = Math.max(BASE_DELAY_MS, Number(process.env.WEBHOOK_MAX_DELAY_MS ?? 2_000));

const subscriptions = new Map<string, WebhookSubscription>();

/** Backoff for attempt N (1-indexed): min(cap, base * 2^(N-1)) + jitter. */
export function retryDelayMs(attempt: number): number {
  const exponential = Math.min(MAX_DELAY_MS, BASE_DELAY_MS * 2 ** Math.max(0, attempt - 1));
  return exponential + Math.floor(Math.random() * BASE_DELAY_MS);
}

/** Canonical signed string: `${timestamp}.${rawBody}`. */
export function signingString(timestamp: number, rawBody: string): string {
  return `${timestamp}.${rawBody}`;
}

/** Header value: `t=<unix seconds>,v1=<hex hmac>`. */
export function signPayload(secret: string, timestamp: number, rawBody: string): string {
  const digest = createHmac('sha256', secret)
    .update(signingString(timestamp, rawBody))
    .digest('hex');
  return `t=${timestamp},v1=${digest}`;
}

/**
 * Constant-time verification helper for receivers (also used in tests).
 * Rejects headers older than `toleranceSeconds` to prevent replays.
 */
export function verifySignature(
  secret: string,
  rawBody: string,
  header: string,
  toleranceSeconds = 300,
  now = Math.floor(Date.now() / 1000)
): boolean {
  const parts = header.split(',');
  const timestampPart = parts.find((p) => p.startsWith('t='));
  const signaturePart = parts.find((p) => p.startsWith('v1='));
  if (!timestampPart || !signaturePart) return false;

  const timestamp = Number(timestampPart.slice(2));
  if (!Number.isFinite(timestamp) || Math.abs(now - timestamp) > toleranceSeconds) {
    return false;
  }

  const expected = createHmac('sha256', secret)
    .update(signingString(timestamp, rawBody))
    .digest('hex');
  const provided = signaturePart.slice(3);
  if (provided.length !== expected.length) return false;
  return timingSafeEqual(Buffer.from(expected), Buffer.from(provided));
}

class WebhookService {
  subscribe(input: {
    url: string;
    events?: string[];
    secret?: string;
  }): { subscription: Omit<WebhookSubscription, 'secret'>; secret: string } {
    if (!/^https?:\/\//.test(input.url)) {
      throw new Error('A valid http(s) webhook URL is required');
    }
    const secret = input.secret ?? randomBytes(32).toString('hex');
    const subscription: WebhookSubscription = {
      id: `whsub_${randomBytes(8).toString('hex')}`,
      url: input.url,
      secret,
      events: input.events && input.events.length > 0 ? input.events : ['*'],
      createdAt: new Date().toISOString(),
      active: true,
    };
    subscriptions.set(subscription.id, subscription);

    const { secret: _secret, ...publicSubscription } = subscription;
    return { subscription: publicSubscription, secret };
  }

  list(): Omit<WebhookSubscription, 'secret'>[] {
    return [...subscriptions.values()].map(({ secret: _secret, ...pub }) => pub);
  }

  unsubscribe(id: string): boolean {
    return subscriptions.delete(id);
  }

  /** Subscriptions interested in an event (`*` matches everything). */
  private subscribersFor(event: string): WebhookSubscription[] {
    return [...subscriptions.values()].filter(
      (s) => s.active && (s.events.includes('*') || s.events.includes(event))
    );
  }

  /**
   * Deliver one event to every matching subscription.
   * Returns per-subscription delivery results; never throws.
   */
  async dispatch(
    event: WebhookEvent,
    data: unknown,
    options: { fetcher?: WebhookFetcher; now?: () => number } = {}
  ): Promise<WebhookDelivery[]> {
    const targets = this.subscribersFor(event);
    if (targets.length === 0) return [];

    const payload: WebhookPayload = {
      id: `evt_${randomBytes(8).toString('hex')}`,
      type: event,
      createdAt: new Date().toISOString(),
      data,
    };
    const rawBody = JSON.stringify(payload);
    const fetcher: WebhookFetcher =
      options.fetcher ??
      ((url, init) => fetch(url, init) as unknown as Promise<{ ok: boolean; status: number }>);

    const results = await Promise.all(
      targets.map((subscription) => this.deliver(subscription, payload, rawBody, fetcher))
    );
    return results;
  }

  private async deliver(
    subscription: WebhookSubscription,
    payload: WebhookPayload,
    rawBody: string,
    fetcher: WebhookFetcher
  ): Promise<WebhookDelivery> {
    const startedAt = Date.now();
    let attempts = 0;
    let lastStatus: number | undefined;
    let lastError: string | undefined;

    for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt += 1) {
      attempts = attempt;
      const timestamp = Math.floor(Date.now() / 1000);
      try {
        const response = await fetcher(subscription.url, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'X-StellarLend-Event': payload.type,
            'X-StellarLend-Delivery': payload.id,
            'X-StellarLend-Signature': signPayload(subscription.secret, timestamp, rawBody),
          },
          body: rawBody,
        });
        lastStatus = response.status;
        if (response.ok) {
          return {
            id: payload.id,
            event: payload.type,
            url: subscription.url,
            status: 'delivered',
            attempts,
            statusCode: response.status,
            durationMs: Date.now() - startedAt,
          };
        }
        lastError = `HTTP ${response.status}`;
      } catch (error: any) {
        lastError = error?.message ?? 'network error';
      }

      if (attempt < MAX_ATTEMPTS) {
        await new Promise((resolve) => setTimeout(resolve, retryDelayMs(attempt)));
      }
    }

    logger.warn('Webhook delivery failed', {
      subscriptionId: subscription.id,
      event: payload.type,
      attempts,
      lastStatus,
      lastError,
    });

    return {
      id: payload.id,
      event: payload.type,
      url: subscription.url,
      status: 'failed',
      attempts,
      statusCode: lastStatus,
      error: lastError,
      durationMs: Date.now() - startedAt,
    };
  }

  /**
   * Issue #1376 – Bridge liquidation monitor alerts to webhook subscribers.
   * Called from the monitor on every position update; no-ops when nobody
   * is subscribed to `liquidation.alert`.
   */
  notifyLiquidationAlerts(alerts: Array<{ address: string; healthFactor: number; netProfit?: number }>): void {
    if (alerts.length === 0 || this.subscribersFor('liquidation.alert').length === 0) return;
    void this.dispatch('liquidation.alert', {
      alerts: alerts.map((a) => ({
        address: a.address,
        healthFactor: a.healthFactor,
        netProfit: a.netProfit ?? 0,
      })),
    }).catch((error) => {
      logger.warn('Liquidation webhook dispatch failed', { error: String(error) });
    });
  }

  /** Exposed for testing only. */
  _reset(): void {
    subscriptions.clear();
  }
}

export const webhookService = new WebhookService();
