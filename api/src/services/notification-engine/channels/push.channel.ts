import * as webpush from 'web-push';
import type { PushSubscription } from 'web-push';
import { config } from '../../../config';
import type { PushConfig } from '../../../config/types';
import { NotificationMessage } from '../../../types/notifications';
import logger from '../../../utils/logger';

/** Push services answer 404/410 once a subscription has expired or been revoked. */
const GONE_STATUS_CODES = new Set([404, 410]);

/** How long a push service keeps retrying an undelivered warning (seconds). */
const PUSH_TTL_SECONDS = 60 * 60;

/**
 * Validates a browser PushSubscription sent as the `recipient` of a push
 * subscription (the JSON string produced by `JSON.stringify(subscription)`).
 */
export function parsePushSubscription(recipient: unknown): PushSubscription | null {
  let value: unknown = recipient;
  if (typeof recipient === 'string') {
    try {
      value = JSON.parse(recipient);
    } catch {
      return null;
    }
  }

  const candidate = value as {
    endpoint?: unknown;
    keys?: { p256dh?: unknown; auth?: unknown };
  } | null;
  const endpoint = candidate?.endpoint;
  const p256dh = candidate?.keys?.p256dh;
  const auth = candidate?.keys?.auth;
  if (
    typeof endpoint !== 'string' ||
    !endpoint.startsWith('https://') ||
    typeof p256dh !== 'string' ||
    typeof auth !== 'string'
  ) {
    return null;
  }

  return { endpoint, keys: { p256dh, auth } };
}

/**
 * Web Push delivery (VAPID). Browsers register through
 * `POST /api/notifications/subscribe` with `channel: "push"`; the service
 * worker shows the notification when it arrives.
 */
export class PushChannel {
  private readonly subscriptions = new Map<string, PushSubscription[]>();
  private vapidReady: boolean | null = null;

  constructor(private readonly vapidOverride?: PushConfig) {}

  /** VAPID public key browsers need to subscribe, or null when push is not configured. */
  getPublicKey(): string | null {
    return this.ensureVapid() ? this.vapidConfig().vapidPublicKey : null;
  }

  addSubscription(userId: string, subscription: PushSubscription): void {
    const others = (this.subscriptions.get(userId) ?? []).filter(
      (existing) => existing.endpoint !== subscription.endpoint
    );
    this.subscriptions.set(userId, [...others, subscription]);
  }

  getSubscriptions(userId: string): PushSubscription[] {
    return [...(this.subscriptions.get(userId) ?? [])];
  }

  async send(message: NotificationMessage): Promise<boolean> {
    try {
      if (!this.ensureVapid()) {
        logger.warn('Push notification skipped: VAPID keys are not configured', {
          id: message.id,
        });
        return false;
      }

      const targets = this.getSubscriptions(message.userId);
      if (targets.length === 0) {
        logger.warn('Push notification skipped: user has no push subscription', {
          id: message.id,
          userId: message.userId,
        });
        return false;
      }

      logger.info('Sending push notification', {
        id: message.id,
        title: message.title,
        userId: message.userId,
      });

      const payload = JSON.stringify({
        id: message.id,
        title: message.title,
        body: message.body,
        alertType: message.alertType,
        data: message.data,
      });

      const results = await Promise.allSettled(
        targets.map((subscription) =>
          webpush.sendNotification(subscription, payload, {
            TTL: PUSH_TTL_SECONDS,
            urgency: 'high',
          })
        )
      );

      let delivered = false;
      results.forEach((result, index) => {
        if (result.status === 'fulfilled') {
          delivered = true;
          return;
        }
        const statusCode = (result.reason as { statusCode?: number } | undefined)?.statusCode;
        const endpoint = targets[index]?.endpoint;
        if (endpoint && statusCode !== undefined && GONE_STATUS_CODES.has(statusCode)) {
          this.removeSubscription(message.userId, endpoint);
        }
        logger.warn('Push delivery failed', { id: message.id, statusCode });
      });

      return delivered;
    } catch (error) {
      logger.error('Push channel failed', { error, messageId: message.id });
      return false;
    }
  }

  private removeSubscription(userId: string, endpoint: string): void {
    const remaining = (this.subscriptions.get(userId) ?? []).filter(
      (subscription) => subscription.endpoint !== endpoint
    );
    if (remaining.length > 0) {
      this.subscriptions.set(userId, remaining);
    } else {
      this.subscriptions.delete(userId);
    }
  }

  private vapidConfig(): PushConfig {
    return this.vapidOverride ?? config.push;
  }

  /** Configures web-push once; push stays disabled when the keys are missing or invalid. */
  private ensureVapid(): boolean {
    if (this.vapidReady !== null) return this.vapidReady;

    const { vapidPublicKey, vapidPrivateKey, vapidSubject } = this.vapidConfig();
    if (!vapidPublicKey || !vapidPrivateKey || !vapidSubject) {
      this.vapidReady = false;
      return false;
    }

    let ready = false;
    try {
      webpush.setVapidDetails(vapidSubject, vapidPublicKey, vapidPrivateKey);
      ready = true;
    } catch (error) {
      logger.error('Invalid VAPID configuration, push notifications disabled', { error });
    }
    this.vapidReady = ready;
    return ready;
  }
}

export const pushChannel = new PushChannel();
