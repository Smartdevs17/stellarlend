/**
 * Prefetch scheduler bootstrap.
 *
 * Pins the entries that must always be warm (protocol stats today) and starts
 * the background refresh loop. Entries that become hot from traffic are
 * picked up automatically by the read cache; see services/prefetch.service.
 */

import { config } from '../config';
import logger from '../utils/logger';
import { prefetchService } from '../services/prefetch.service';
import { StellarService } from '../services/stellar.service';

export function registerPinnedLoaders(): void {
  prefetchService.registerLoader('protocol', 'stats', config.cache.protocolStatsTtlMs, () =>
    new StellarService().loadProtocolStats()
  );
}

export function startPrefetchScheduler(): boolean {
  if (!config.prefetch.enabled) {
    logger.info('Prefetch scheduler disabled (PREFETCH_ENABLED=false)');
    return false;
  }
  registerPinnedLoaders();
  return prefetchService.start();
}

export function stopPrefetchScheduler(): void {
  prefetchService.stop();
}
