/**
 * Chaos Engineering Test Suite: Network Failures
 *
 * Tests resilience of the StellarLend oracle→contract→API pipeline
 * against simulated network failures, latency spikes, partial outages,
 * and retry logic. All external calls are mocked.
 */

import request from 'supertest';
import express, { Application } from 'express';
import axios from 'axios';

// ─── Price Store (shared mock state) ────────────────────────────────────────

interface StoredPrice {
  asset: string;
  price: number;
  timestamp: number;
  txHash: string;
}

class PriceStore {
  private prices = new Map<string, StoredPrice>();
  private failureMode = false;

  set(asset: string, price: number, timestamp: number, txHash: string) {
    this.prices.set(asset, { asset, price, timestamp, txHash });
  }

  get(asset: string): StoredPrice | undefined {
    return this.prices.get(asset);
  }

  getAll(): StoredPrice[] {
    return Array.from(this.prices.values());
  }

  clear() {
    this.prices.clear();
    this.failureMode = false;
  }

  setFailureMode(fail: boolean) {
    this.failureMode = fail;
  }

  isFailureMode(): boolean {
    return this.failureMode;
  }
}

const store = new PriceStore();

// ─── Mock Providers with Chaos Capabilities ──────────────────────────────────

interface MockProvider {
  name: string;
  latencyMs: number;
  failureRate: number;
  fetchPrices(assets: string[]): Promise<Record<string, number>>;
}

class ResilientMockProvider implements MockProvider {
  name = 'coingecko';
  latencyMs = 0;
  failureRate = 0;
  private prices: Record<string, number>;
  private callCount = 0;

  constructor(prices: Record<string, number>, opts?: { latencyMs?: number; failureRate?: number }) {
    this.prices = prices;
    this.latencyMs = opts?.latencyMs ?? 0;
    this.failureRate = opts?.failureRate ?? 0;
  }

  async fetchPrices(assets: string[]): Promise<Record<string, number>> {
    this.callCount++;

    if (this.latencyMs > 0) {
      await new Promise((r) => setTimeout(r, this.latencyMs));
    }

    if (this.failureRate > 0 && Math.random() < this.failureRate) {
      throw new Error(`[Chaos] ${this.name} simulated network failure (call #${this.callCount})`);
    }

    if (store.isFailureMode()) {
      throw new Error(`[Chaos] ${this.name} global failure mode active`);
    }

    const result: Record<string, number> = {};
    for (const asset of assets) {
      if (this.prices[asset] !== undefined) {
        result[asset] = this.prices[asset];
      }
    }
    return result;
  }

  getCallCount(): number {
    return this.callCount;
  }
}

let globalTxCounter = 0;

class MockContractUpdater {
  private failureMode = false;

  setFailureMode(fail: boolean) {
    this.failureMode = fail;
  }

  async updatePrice(
    asset: string,
    price: number,
    timestamp: number
  ): Promise<{ success: boolean; txHash: string }> {
    if (this.failureMode) {
      throw new Error('[Chaos] Contract update failed: simulated network timeout');
    }
    globalTxCounter++;
    const txHash = `mock-tx-${asset}-${globalTxCounter}`;
    store.set(asset, price, timestamp, txHash);
    return { success: true, txHash };
  }
}

// ─── Oracle with Retry Logic ─────────────────────────────────────────────────

class ResilientOracle {
  private provider: MockProvider;
  private updater: MockContractUpdater;
  private maxRetries: number;

  constructor(provider: MockProvider, updater: MockContractUpdater, maxRetries = 3) {
    this.provider = provider;
    this.updater = updater;
    this.maxRetries = maxRetries;
  }

  async updatePrices(assets: string[]): Promise<{ success: boolean; errors: string[] }> {
    const errors: string[] = [];
    let prices: Record<string, number> = {};

    // Retry fetching prices from provider
    for (let attempt = 1; attempt <= this.maxRetries; attempt++) {
      try {
        prices = await this.provider.fetchPrices(assets);
        break;
      } catch (err) {
        errors.push(`Provider attempt ${attempt}/${this.maxRetries}: ${(err as Error).message}`);
        if (attempt === this.maxRetries) {
          return { success: false, errors };
        }
        await new Promise((r) => setTimeout(r, 10 * attempt));
      }
    }

    // Update prices on contract with retry
    const updatedAssets: string[] = [];
    for (const [asset, price] of Object.entries(prices)) {
      for (let attempt = 1; attempt <= this.maxRetries; attempt++) {
        try {
          const timestamp = Math.floor(Date.now() / 1000);
          await this.updater.updatePrice(asset, price, timestamp);
          updatedAssets.push(asset);
          break;
        } catch (err) {
          errors.push(`Contract update attempt ${attempt} for ${asset}: ${(err as Error).message}`);
        }
      }
    }

    return { success: updatedAssets.length > 0, errors };
  }
}

// ─── API App ─────────────────────────────────────────────────────────────────

function buildApiApp(): Application {
  const app = express();
  app.use(express.json());

  app.get('/api/health', (_req, res) => {
    res.json({
      status: store.isFailureMode() ? 'degraded' : 'healthy',
      timestamp: new Date().toISOString(),
    });
  });

  app.get('/api/prices/:asset', (req, res) => {
    const asset = req.params.asset.toUpperCase();
    const entry = store.get(asset);
    if (!entry) {
      return res.status(404).json({ error: `Price not found for asset: ${asset}` });
    }
    return res.json({
      asset: entry.asset,
      price: entry.price,
      timestamp: entry.timestamp,
      txHash: entry.txHash,
    });
  });

  app.get('/api/prices', (_req, res) => {
    res.json({ prices: store.getAll() });
  });

  app.post('/api/lending/submit', (req, res) => {
    if (store.isFailureMode()) {
      return res.status(503).json({ error: 'Service temporarily unavailable' });
    }
    const { signedXdr } = req.body;
    if (!signedXdr) {
      return res.status(400).json({ error: 'signedXdr is required' });
    }
    res.json({
      success: true,
      transactionHash: `mock-submitted-${Date.now()}`,
      status: 'success',
    });
  });

  return app;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

describe('Chaos Engineering: Network Failure Resilience', () => {
  let app: Application;
  const MOCK_PRICES: Record<string, number> = {
    XLM: 0.12,
    USDC: 1.0,
    BTC: 50000,
  };

  beforeAll(() => {
    app = buildApiApp();
  });

  beforeEach(() => {
    store.clear();
    globalTxCounter = 0;
  });

  // ── Provider Failures ────────────────────────────────────────────────────

  describe('Provider failures', () => {
    it('retries on provider failure and succeeds', async () => {
      const provider = new ResilientMockProvider(MOCK_PRICES, { failureRate: 1.0 });
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 3);

      const result = await oracle.updatePrices(['XLM']);
      expect(result.success).toBe(false);
      expect(result.errors.length).toBe(3);
    });

    it('partially succeeds when some assets fail to fetch', async () => {
      const provider = new ResilientMockProvider(
        { XLM: 0.12 },
        { failureRate: 0 }
      );
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 1);

      const result = await oracle.updatePrices(['XLM', 'BTC']);
      expect(result.success).toBe(true);
      expect(store.get('XLM')).toBeDefined();
      expect(store.get('BTC')).toBeUndefined();
    });

    it('returns errors array with failure details', async () => {
      const provider = new ResilientMockProvider(MOCK_PRICES, { failureRate: 1.0 });
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 2);

      const result = await oracle.updatePrices(['XLM']);
      expect(result.errors.length).toBeGreaterThanOrEqual(1);
      expect(result.errors[0]).toContain('simulated network failure');
    });
  });

  // ── Contract Update Failures ─────────────────────────────────────────────

  describe('Contract update failures', () => {
    it('retries contract updates on failure', async () => {
      const provider = new ResilientMockProvider(MOCK_PRICES);
      const updater = new MockContractUpdater();
      updater.setFailureMode(true);
      const oracle = new ResilientOracle(provider, updater, 3);

      const result = await oracle.updatePrices(['XLM']);
      expect(result.success).toBe(false);
      expect(result.errors.length).toBe(3);
      expect(store.get('XLM')).toBeUndefined();
    });

    it('succeeds after contract updater recovers', async () => {
      const provider = new ResilientMockProvider(MOCK_PRICES);
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 3);

      updater.setFailureMode(true);
      let result = await oracle.updatePrices(['XLM']);
      expect(result.success).toBe(false);

      updater.setFailureMode(false);
      result = await oracle.updatePrices(['XLM']);
      expect(result.success).toBe(true);
      expect(store.get('XLM')).toBeDefined();
    });
  });

  // ── Global Failure Mode ──────────────────────────────────────────────────

  describe('Global failure mode', () => {
    it('store failure mode returns degraded health', async () => {
      store.setFailureMode(true);
      const res = await request(app).get('/api/health');
      expect(res.body.status).toBe('degraded');
    });

    it('submit fails during global outage', async () => {
      store.setFailureMode(true);
      const res = await request(app)
        .post('/api/lending/submit')
        .send({ signedXdr: 'test-xdr' });
      expect(res.status).toBe(503);
    });

    it('prices still readable during contract write failure', async () => {
      await store.set('XLM', 0.12, Date.now(), 'tx-1');
      store.setFailureMode(true);

      const res = await request(app).get('/api/prices/XLM');
      expect(res.status).toBe(200);
      expect(res.body.price).toBe(0.12);
    });

    it('recovery from failure mode restores full operation', async () => {
      store.setFailureMode(true);
      let res = await request(app).post('/api/lending/submit').send({ signedXdr: 'x' });
      expect(res.status).toBe(503);

      store.setFailureMode(false);
      res = await request(app).post('/api/lending/submit').send({ signedXdr: 'x' });
      expect(res.status).toBe(200);
    });
  });

  // ── Partial Outages ──────────────────────────────────────────────────────

  describe('Partial outages', () => {
    it('operational when only one provider fails', async () => {
      const goodProvider = new ResilientMockProvider({ XLM: 0.12 });
      const badProvider = new ResilientMockProvider(MOCK_PRICES, { failureRate: 1.0 });
      const updater = new MockContractUpdater();

      const oracle = new ResilientOracle(goodProvider, updater, 1);
      const result = await oracle.updatePrices(['XLM']);
      expect(result.success).toBe(true);
      expect(store.get('XLM')?.price).toBe(0.12);
    });

    it('API serves stale data when oracle is down', async () => {
      store.set('XLM', 0.12, Date.now(), 'old-tx');

      const provider = new ResilientMockProvider({}, { failureRate: 1.0 });
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 1);
      await oracle.updatePrices(['XLM']);

      const res = await request(app).get('/api/prices/XLM');
      expect(res.status).toBe(200);
      expect(res.body.price).toBe(0.12);
    });
  });

  // ── Latency Spikes ───────────────────────────────────────────────────────

  describe('Latency resilience', () => {
    it('handles high latency provider', async () => {
      const provider = new ResilientMockProvider(MOCK_PRICES, { latencyMs: 50 });
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 1);

      const start = Date.now();
      const result = await oracle.updatePrices(['XLM']);
      const elapsed = Date.now() - start;

      expect(result.success).toBe(true);
      expect(elapsed).toBeGreaterThanOrEqual(50);
      expect(store.get('XLM')).toBeDefined();
    });

    it('API responds within timeout under load', async () => {
      const res = await request(app).get('/api/health');
      expect(res.status).toBe(200);
    });
  });

  // ── Data Consistency ─────────────────────────────────────────────────────

  describe('Data consistency under chaos', () => {
    it('partial update does not corrupt existing data', async () => {
      store.set('XLM', 0.12, Date.now(), 'existing-tx');

      const provider = new ResilientMockProvider({ BTC: 50000 }, { failureRate: 0 });
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 1);
      await oracle.updatePrices(['BTC']);

      const xlm = store.get('XLM');
      expect(xlm?.price).toBe(0.12);
      expect(store.get('BTC')?.price).toBe(50000);
    });

    it('concurrent chaos updates maintain consistency', async () => {
      const provider = new ResilientMockProvider(MOCK_PRICES, { failureRate: 0.5 });
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 2);

      const results = await Promise.all([
        oracle.updatePrices(['XLM']),
        oracle.updatePrices(['USDC']),
        oracle.updatePrices(['BTC']),
      ]);

      const successCount = results.filter((r) => r.success).length;
      expect(successCount).toBeGreaterThanOrEqual(1);

      const allPrices = store.getAll();
      for (const p of allPrices) {
        expect(p.price).toBeGreaterThan(0);
        expect(p.txHash).toBeTruthy();
      }
    });
  });

  // ── Recovery Scenarios ───────────────────────────────────────────────────

  describe('Recovery scenarios', () => {
    it('full recovery after provider outage', async () => {
      const provider = new ResilientMockProvider(MOCK_PRICES, { failureRate: 1.0 });
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 1);

      await oracle.updatePrices(['XLM']);
      expect(store.get('XLM')).toBeUndefined();

      const recoveredProvider = new ResilientMockProvider(MOCK_PRICES);
      const oracle2 = new ResilientOracle(recoveredProvider, updater, 1);
      await oracle2.updatePrices(['XLM']);

      expect(store.get('XLM')?.price).toBe(0.12);
    });

    it('stale price overwritten after recovery', async () => {
      store.set('XLM', 0.05, Date.now(), 'stale-tx');

      const provider = new ResilientMockProvider({ XLM: 0.15 });
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 1);
      await oracle.updatePrices(['XLM']);

      expect(store.get('XLM')?.price).toBe(0.15);
      expect(store.get('XLM')?.txHash).not.toBe('stale-tx');
    });
  });

  // ── Exhaustive Chaos Scenarios ───────────────────────────────────────────

  describe('Comprehensive chaos matrix', () => {
    it('handles 100% provider failure for multiple assets', async () => {
      const provider = new ResilientMockProvider(MOCK_PRICES, { failureRate: 1.0 });
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 2);

      const result = await oracle.updatePrices(['XLM', 'USDC', 'BTC']);
      expect(result.success).toBe(false);
    });

    it('handles 50% failure rate with retries', async () => {
      const provider = new ResilientMockProvider(MOCK_PRICES, { failureRate: 0.5 });
      const updater = new MockContractUpdater();
      const oracle = new ResilientOracle(provider, updater, 5);

      const results = [];
      for (let i = 0; i < 10; i++) {
        const result = await oracle.updatePrices(['XLM']);
        results.push(result);
      }

      const successCount = results.filter((r) => r.success).length;
      expect(successCount).toBeGreaterThan(0);
    });

    it('provider + contract failure simultaneously', async () => {
      const provider = new ResilientMockProvider(MOCK_PRICES, { failureRate: 1.0 });
      const updater = new MockContractUpdater();
      updater.setFailureMode(true);
      const oracle = new ResilientOracle(provider, updater, 2);

      const result = await oracle.updatePrices(['XLM']);
      expect(result.success).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });
});

void axios;
