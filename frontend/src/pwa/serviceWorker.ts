import { useEffect, useState } from 'react';

/** `frontend/public/sw.js` is served from the site root. */
export const SERVICE_WORKER_URL = '/sw.js';

/**
 * Registers the StellarLend service worker, which provides offline caching and
 * shows push notifications. Call once from the app entry point. Resolves to
 * null where service workers are unavailable (unsupported browser or an
 * insecure origin).
 */
export async function registerServiceWorker(
  url: string = SERVICE_WORKER_URL
): Promise<ServiceWorkerRegistration | null> {
  if (typeof navigator === 'undefined' || !('serviceWorker' in navigator)) return null;
  try {
    return await navigator.serviceWorker.register(url, { scope: '/' });
  } catch (error) {
    console.error('Service worker registration failed:', error);
    return null;
  }
}

/** Tracks whether the browser currently has a network connection. */
export function useOnlineStatus(): boolean {
  const [online, setOnline] = useState(() =>
    typeof navigator === 'undefined' ? true : navigator.onLine
  );

  useEffect(() => {
    const update = () => setOnline(navigator.onLine);
    window.addEventListener('online', update);
    window.addEventListener('offline', update);
    return () => {
      window.removeEventListener('online', update);
      window.removeEventListener('offline', update);
    };
  }, []);

  return online;
}
