import { registerServiceWorker } from './serviceWorker';

/** Alert types the API sends as liquidation warnings. */
export const LIQUIDATION_WARNING_ALERTS = ['health_factor_low', 'approaching_liquidation'];

/** Converts the URL-safe base64 VAPID public key into the bytes PushManager expects. */
function urlBase64ToUint8Array(base64: string): Uint8Array {
  const padding = '='.repeat((4 - (base64.length % 4)) % 4);
  const raw = atob((base64 + padding).replace(/-/g, '+').replace(/_/g, '/'));
  return Uint8Array.from(raw, (char) => char.charCodeAt(0));
}

export function isPushSupported(): boolean {
  return (
    typeof window !== 'undefined' &&
    'serviceWorker' in navigator &&
    'PushManager' in window &&
    'Notification' in window
  );
}

/**
 * Asks for notification permission, subscribes this browser to Web Push and
 * registers the subscription with the API so the user receives liquidation
 * warnings. Throws with a user-facing message when that is not possible.
 */
export async function subscribeToLiquidationWarnings(userAddress: string): Promise<void> {
  if (!isPushSupported()) {
    throw new Error('This browser does not support push notifications');
  }

  const permission = await Notification.requestPermission();
  if (permission !== 'granted') {
    throw new Error('Notification permission was not granted');
  }

  if (!(await registerServiceWorker())) {
    throw new Error('The service worker could not be registered');
  }
  const registration = await navigator.serviceWorker.ready;

  const keyRes = await fetch('/api/notifications/push/public-key');
  const keyBody = await keyRes.json();
  if (!keyBody.success) {
    throw new Error(keyBody.error || 'Push notifications are not available');
  }

  const subscription =
    (await registration.pushManager.getSubscription()) ??
    (await registration.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: urlBase64ToUint8Array(keyBody.data.publicKey),
    }));

  const res = await fetch('/api/notifications/subscribe', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'x-user-address': userAddress },
    body: JSON.stringify({
      channel: 'push',
      recipient: JSON.stringify(subscription),
      alertTypes: LIQUIDATION_WARNING_ALERTS,
    }),
  });
  const body = await res.json();
  if (!body.success) {
    throw new Error(body.error || 'Failed to register for liquidation warnings');
  }
}
