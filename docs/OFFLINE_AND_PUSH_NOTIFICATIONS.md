# Offline Support and Liquidation-Warning Push Notifications

This covers the browser side of StellarLend: a service worker that keeps the
app usable offline, and Web Push notifications that warn users before their
position is liquidated.

## Service worker (`frontend/public/sw.js`)

Serve the file from the site root (`/sw.js`) and register it once when the app
starts:

```ts
import { registerServiceWorker } from './pwa/serviceWorker';

registerServiceWorker();
```

Caching:

| Request | Strategy |
| --- | --- |
| Page navigations | Network first. Offline, the cached page or app shell is served, or a small offline page if nothing was cached. |
| `GET /api/*` | Network first. Offline, the last successful response is served. Responses marked `Cache-Control: no-store` are never stored. When there is no cached copy, the reply is `503 { success: false, error: "You are offline" }`, the same shape as other API errors. |
| Other same-origin assets | Served from the cache immediately and refreshed in the background. |
| Anything that is not a `GET` | Never intercepted. Transactions always go to the network and are never cached or replayed. |

Bump `CACHE_VERSION` in `sw.js` to drop old caches on the next activation.

`OfflineStatusBanner` (built on the `useOnlineStatus` hook) tells the user
when they are offline.

## Push notifications for liquidation warnings

### Server setup

Generate a VAPID key pair once and set it in the API environment:

```bash
npx web-push generate-vapid-keys
```

```env
VAPID_PUBLIC_KEY=<public key>
VAPID_PRIVATE_KEY=<private key>
VAPID_SUBJECT=mailto:ops@example.com
```

Until all three are set, `GET /api/notifications/push/public-key` returns
`503` and push delivery is skipped.

### Opting in

`LiquidationWarningPushToggle`, or `subscribeToLiquidationWarnings(address)`
from `frontend/src/pwa/pushNotifications.ts`:

1. asks for notification permission and registers the service worker
2. fetches the VAPID public key from `GET /api/notifications/push/public-key`
3. subscribes the browser with `PushManager`
4. sends the subscription to `POST /api/notifications/subscribe` with
   `channel: "push"`, `recipient: JSON.stringify(subscription)` and the
   `health_factor_low` / `approaching_liquidation` alert types. The user is
   identified by the `x-user-address` header.

### When warnings are sent

`startLiquidationWarnings()` runs from `api/src/index.ts`. It listens to the
collateral ratio monitor's `position_update` events. The bands below use the default risk thresholds:

| Position risk level | Alert |
| --- | --- |
| `danger` (health factor from 1.1 up to 1.5) | `health_factor_low` |
| `critical` (health factor below 1.1) | `approaching_liquidation` |

The notification engine delivers each alert to every channel the user
subscribed to. It limits repeats of the same alert to one per 5 minutes.

The push channel signs messages with the VAPID keys and sends them with high
urgency and a one-hour TTL. It removes subscriptions that the push service
reports as expired (`404`/`410`).

In the browser, the service worker shows the notification. There is one per
alert type, so a newer warning replaces the older one, and
`approaching_liquidation` stays on screen until it is dismissed. Clicking a
notification focuses or opens the app.
