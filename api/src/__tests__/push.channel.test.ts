jest.mock('web-push', () => ({
  setVapidDetails: jest.fn(),
  sendNotification: jest.fn(),
}));

import * as webpush from 'web-push';
import {
  PushChannel,
  parsePushSubscription,
} from '../services/notification-engine/channels/push.channel';
import type { NotificationMessage } from '../types/notifications';

const mockedWebpush = webpush as jest.Mocked<typeof webpush>;

const vapid = {
  vapidPublicKey: 'public-key',
  vapidPrivateKey: 'private-key',
  vapidSubject: 'mailto:ops@example.com',
};
const user = 'GUSER';
const subscription = {
  endpoint: 'https://push.example.com/subscriptions/1',
  keys: { p256dh: 'p256dh-key', auth: 'auth-secret' },
};

function message(overrides: Partial<NotificationMessage> = {}): NotificationMessage {
  return {
    id: 'msg-1',
    userId: user,
    channel: 'push',
    alertType: 'approaching_liquidation',
    title: '🚨 Approaching Liquidation',
    body: 'Your position is approaching liquidation (HF: 1.05).',
    data: { asset: 'XLM' },
    status: 'pending',
    createdAt: new Date(0).toISOString(),
    ...overrides,
  };
}

beforeEach(() => {
  jest.clearAllMocks();
  mockedWebpush.sendNotification.mockResolvedValue({ statusCode: 201, body: '', headers: {} });
});

describe('parsePushSubscription', () => {
  it('accepts a JSON-encoded browser push subscription', () => {
    const encoded = JSON.stringify({ ...subscription, expirationTime: null });
    expect(parsePushSubscription(encoded)).toEqual(subscription);
  });

  it('rejects malformed, incomplete or non-https subscriptions', () => {
    expect(parsePushSubscription('user@example.com')).toBeNull();
    expect(
      parsePushSubscription(JSON.stringify({ ...subscription, endpoint: 'http://push.example.com' }))
    ).toBeNull();
    expect(
      parsePushSubscription(
        JSON.stringify({ endpoint: subscription.endpoint, keys: { auth: 'auth-secret' } })
      )
    ).toBeNull();
  });
});

describe('PushChannel', () => {
  it('delivers the alert to every push subscription of the user', async () => {
    const channel = new PushChannel(vapid);
    channel.addSubscription(user, subscription);

    await expect(channel.send(message())).resolves.toBe(true);

    expect(mockedWebpush.setVapidDetails).toHaveBeenCalledWith(
      'mailto:ops@example.com',
      'public-key',
      'private-key'
    );
    expect(mockedWebpush.sendNotification).toHaveBeenCalledTimes(1);
    const [target, payload, options] = mockedWebpush.sendNotification.mock.calls[0]!;
    expect(target).toEqual(subscription);
    expect(JSON.parse(payload as string)).toMatchObject({
      id: 'msg-1',
      title: '🚨 Approaching Liquidation',
      alertType: 'approaching_liquidation',
      data: { asset: 'XLM' },
    });
    expect(options).toMatchObject({ urgency: 'high' });
  });

  it('does not send when VAPID keys are not configured', async () => {
    const channel = new PushChannel({ vapidPublicKey: '', vapidPrivateKey: '', vapidSubject: '' });
    channel.addSubscription(user, subscription);

    await expect(channel.send(message())).resolves.toBe(false);
    expect(channel.getPublicKey()).toBeNull();
    expect(mockedWebpush.sendNotification).not.toHaveBeenCalled();
  });

  it('returns false when the user has no push subscription', async () => {
    const channel = new PushChannel(vapid);

    await expect(channel.send(message())).resolves.toBe(false);
    expect(mockedWebpush.sendNotification).not.toHaveBeenCalled();
  });

  it('drops a subscription the push service reports as gone', async () => {
    const channel = new PushChannel(vapid);
    channel.addSubscription(user, subscription);
    mockedWebpush.sendNotification.mockRejectedValueOnce(
      Object.assign(new Error('Subscription expired'), { statusCode: 410 })
    );

    await expect(channel.send(message())).resolves.toBe(false);
    expect(channel.getSubscriptions(user)).toEqual([]);
  });

  it('replaces a re-registered subscription instead of duplicating it', () => {
    const channel = new PushChannel(vapid);
    channel.addSubscription(user, subscription);
    channel.addSubscription(user, { ...subscription, keys: { p256dh: 'rotated', auth: 'rotated' } });

    expect(channel.getSubscriptions(user)).toHaveLength(1);
    expect(channel.getSubscriptions(user)[0]!.keys.p256dh).toBe('rotated');
  });

  it('exposes the VAPID public key when push is configured', () => {
    expect(new PushChannel(vapid).getPublicKey()).toBe('public-key');
  });
});
