/*
 * StellarLend service worker.
 *
 * - Offline support: the app shell and static assets are cached, and GET
 *   /api/* responses are kept so the last loaded data stays readable offline.
 *   Anything that is not a GET (e.g. transaction submission) always goes to
 *   the network and is never cached or replayed.
 * - Push notifications: shows the liquidation warnings the API sends through
 *   Web Push and focuses the app when one is clicked.
 *
 * Served from the site root (`/sw.js`) so its scope covers the whole app.
 */

const CACHE_VERSION = 'v1';
const STATIC_CACHE = `stellarlend-static-${CACHE_VERSION}`;
const API_CACHE = `stellarlend-api-${CACHE_VERSION}`;

// Entry points precached on install so the app can open offline.
const APP_SHELL = ['/', '/index.html'];

const OFFLINE_PAGE = `<!doctype html>
<html lang="en">
  <head><meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" /><title>StellarLend — offline</title></head>
  <body style="font-family: sans-serif; padding: 24px; text-align: center">
    <h1>You are offline</h1>
    <p>StellarLend will reload your data as soon as the connection is back.</p>
  </body>
</html>`;

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(STATIC_CACHE)
      // Cache each entry separately: a shell path the host app does not serve
      // must not block installation.
      .then((cache) => Promise.allSettled(APP_SHELL.map((url) => cache.add(url))))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys
            .filter((key) => key.startsWith('stellarlend-') && key !== STATIC_CACHE && key !== API_CACHE)
            .map((key) => caches.delete(key))
        )
      )
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  const { request } = event;
  if (request.method !== 'GET') return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  if (url.pathname.startsWith('/api/')) {
    event.respondWith(networkFirst(request, API_CACHE));
  } else if (request.mode === 'navigate') {
    event.respondWith(networkFirst(request, STATIC_CACHE));
  } else {
    event.respondWith(staleWhileRevalidate(event));
  }
});

/** Network first, falling back to the last cached response when offline. */
async function networkFirst(request, cacheName) {
  const cache = await caches.open(cacheName);
  try {
    const response = await fetch(request);
    const cacheControl = response.headers.get('Cache-Control') || '';
    if (response.ok && !/no-store/i.test(cacheControl)) {
      cache.put(request, response.clone());
    }
    return response;
  } catch {
    const cached = await cache.match(request);
    if (cached) return cached;

    if (request.mode === 'navigate') {
      const shell = await caches.match('/');
      return (
        shell ||
        new Response(OFFLINE_PAGE, { headers: { 'Content-Type': 'text/html; charset=utf-8' } })
      );
    }

    // Same shape as API errors, so components show the message as usual.
    return new Response(JSON.stringify({ success: false, error: 'You are offline', offline: true }), {
      status: 503,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}

/** Serve static assets from the cache immediately and refresh them in the background. */
function staleWhileRevalidate(event) {
  const refresh = fetch(event.request).then((response) => {
    if (response.ok) {
      const copy = response.clone();
      caches.open(STATIC_CACHE).then((cache) => cache.put(event.request, copy));
    }
    return response;
  });
  event.waitUntil(refresh.then(() => undefined, () => undefined));

  return caches
    .match(event.request)
    .then((cached) => cached || refresh.catch(() => Response.error()));
}

self.addEventListener('push', (event) => {
  let payload = {};
  if (event.data) {
    try {
      payload = event.data.json();
    } catch {
      payload = { body: event.data.text() };
    }
  }

  const alertType = payload.alertType || 'stellarlend-alert';
  event.waitUntil(
    self.registration.showNotification(payload.title || 'StellarLend alert', {
      body: payload.body || '',
      // One notification per alert type; a newer warning replaces the old one.
      tag: alertType,
      renotify: true,
      requireInteraction: alertType === 'approaching_liquidation',
      data: { url: (payload.data && payload.data.url) || '/', alertType, id: payload.id },
    })
  );
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const target = new URL(
    (event.notification.data && event.notification.data.url) || '/',
    self.location.origin
  ).href;

  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((windows) => {
      const open = windows.find((client) => client.url === target);
      return open ? open.focus() : self.clients.openWindow(target);
    })
  );
});
