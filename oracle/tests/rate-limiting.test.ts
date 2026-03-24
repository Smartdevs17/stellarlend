/**
 * Load tests for provider rate limiting under concurrent requests.
 */

import { describe, it, expect } from 'vitest';
import { BasePriceProvider } from '../src/providers/base-provider.js';
import type { RawPriceData } from '../src/types/index.js';

class RateLimitTestProvider extends BasePriceProvider {
  public completionTimes: number[] = [];

  constructor(maxRequests: number, windowMs: number) {
    super({
      name: 'rate-limit-test-provider',
      enabled: true,
      priority: 1,
      weight: 1,
      baseUrl: 'https://mock.api',
      rateLimit: { maxRequests, windowMs },
    });
  }

  async fetchPrice(asset: string): Promise<RawPriceData> {
    await this.enforceRateLimit();
    this.completionTimes.push(Date.now());
    return {
      asset: asset.toUpperCase(),
      price: 1,
      timestamp: Math.floor(Date.now() / 1000),
      source: this.name,
    };
  }
}

describe('Oracle rate limiting load tests', () => {
  it('queues concurrent requests above configured rate limit', async () => {
    const provider = new RateLimitTestProvider(5, 50);
    const start = Date.now();

    await Promise.all(Array.from({ length: 20 }, () => provider.fetchPrice('XLM')));

    const elapsed = Date.now() - start;
    expect(provider.completionTimes).toHaveLength(20);
    // At least one batch must be deferred beyond the initial window.
    const deferredCount = provider.completionTimes.filter((t) => t - start >= 50).length;
    expect(deferredCount).toBeGreaterThan(0);
    expect(elapsed).toBeGreaterThanOrEqual(50);
  });

  it('does not allow more than maxRequests in the first window', async () => {
    const maxRequests = 4;
    const windowMs = 60;
    const provider = new RateLimitTestProvider(maxRequests, windowMs);
    const start = Date.now();

    await Promise.all(Array.from({ length: 12 }, () => provider.fetchPrice('XLM')));

    const inFirstWindow = provider.completionTimes.filter((t) => t - start < windowMs).length;
    expect(inFirstWindow).toBeLessThanOrEqual(maxRequests);
    expect(provider.completionTimes).toHaveLength(12);
  });
});
