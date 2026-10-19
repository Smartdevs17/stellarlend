/**
 * Chaos Engineering Test Suite: Network Partition Simulation (Issue #689)
 *
 * Deterministic simulation of component-to-component network partitions:
 * - API ⇄ RPC partition (submission path severed, reads continue)
 * - Oracle ⇄ Contract partition (price feed severed, staleness enforced)
 * - Partial endpoint partitions (subset of methods unreachable)
 * - Split-brain healing and post-heal consistency verification
 *
 * Unlike `network-failures.test.ts` (which models probabilistic endpoint
 * failures), this suite models *partitions*: a hard, deterministic split
 * between two components that later heals.
 */

interface Component {
  name: string;
  reachable: boolean;
}

type PartitionSide = 'api-rpc' | 'oracle-contract' | 'partial-endpoint';

interface PartitionRecord {
  side: PartitionSide;
  isolated: string[];
  startMs: number;
  healedAtMs: number | null;
}

class NetworkPartitionSimulator {
  private components = new Map<string, Component>();
  private partitions: PartitionRecord[] = [];
  private activePartition: PartitionRecord | null = null;
  private endpointHealth = new Map<string, boolean>();
  private oracleFeed: Map<string, { price: number; updatedAtMs: number }> = new Map();
  private acceptedPrices = new Map<string, number>();
  private readonly staleThresholdMs = 60_000;
  private clockMs = 0;

  constructor() {
    for (const name of ['api', 'rpc', 'oracle', 'contract', 'indexer']) {
      this.components.set(name, { name, reachable: true });
    }
    for (const endpoint of [
      'get-ledger-entry',
      'simulate-transaction',
      'submit-transaction',
      'get-health',
    ]) {
      this.endpointHealth.set(endpoint, true);
    }
    this.oracleFeed.set('XLM', { price: 0.12, updatedAtMs: this.clockMs });
    this.oracleFeed.set('USDC', { price: 1.0, updatedAtMs: this.clockMs });
  }

  /** Advance the simulated clock without real sleeps. */
  advanceClock(ms: number): void {
    this.clockMs += ms;
  }

  now(): number {
    return this.clockMs;
  }

  isolate(side: PartitionSide): PartitionRecord {
    if (this.activePartition) {
      throw new Error('a partition is already active — heal it first');
    }
    const isolated: string[] = [];
    if (side === 'api-rpc') {
      isolated.push('rpc');
      this.setReachable('rpc', false);
      this.endpointHealth.set('submit-transaction', false);
      this.endpointHealth.set('simulate-transaction', false);
    } else if (side === 'oracle-contract') {
      isolated.push('oracle');
      this.setReachable('oracle', false);
    } else if (side === 'partial-endpoint') {
      isolated.push('submit-transaction');
      this.endpointHealth.set('submit-transaction', false);
    }
    const record: PartitionRecord = {
      side,
      isolated,
      startMs: this.clockMs,
      healedAtMs: null,
    };
    this.partitions.push(record);
    this.activePartition = record;
    return record;
  }

  heal(): PartitionRecord {
    const record = this.activePartition;
    if (!record) {
      throw new Error('no active partition to heal');
    }
    for (const name of ['api', 'rpc', 'oracle', 'contract', 'indexer']) {
      this.setReachable(name, true);
    }
    for (const endpoint of this.endpointHealth.keys()) {
      this.endpointHealth.set(endpoint, true);
    }
    record.healedAtMs = this.clockMs;
    this.activePartition = null;
    return record;
  }

  isPartitionActive(): boolean {
    return this.activePartition !== null;
  }

  canCall(from: string, to: string): boolean {
    const a = this.components.get(from);
    const b = this.components.get(to);
    if (!a || !b) return false;
    return a.reachable && b.reachable;
  }

  callEndpoint(endpoint: string): { ok: boolean; error?: string } {
    const healthy = this.endpointHealth.get(endpoint);
    if (healthy === false) {
      return { ok: false, error: `endpoint ${endpoint} unreachable (partition)` };
    }
    if (!this.canCall('api', 'rpc') && endpoint !== 'get-health') {
      return { ok: false, error: 'api ⇄ rpc partition active' };
    }
    return { ok: true };
  }

  /** Feed a price from the oracle; returns false if the feed is partitioned. */
  pushPrice(asset: string, price: number): boolean {
    if (!this.canCall('oracle', 'contract')) {
      return false;
    }
    this.oracleFeed.set(asset, { price, updatedAtMs: this.clockMs });
    return true;
  }

  /**
   * Contract-side acceptance: reject prices that are stale relative to the
   * simulated clock (partition froze the feed while wall time advanced).
   */
  contractAcceptsPrice(asset: string): { accepted: boolean; reason?: string } {
    const feed = this.oracleFeed.get(asset);
    if (!feed) {
      return { accepted: false, reason: 'no price feed' };
    }
    const age = this.clockMs - feed.updatedAtMs;
    if (age > this.staleThresholdMs) {
      return { accepted: false, reason: `stale price (${age}ms > ${this.staleThresholdMs}ms)` };
    }
    this.acceptedPrices.set(asset, feed.price);
    return { accepted: true };
  }

  getAcceptedPrice(asset: string): number | undefined {
    return this.acceptedPrices.get(asset);
  }

  priceAgeMs(asset: string): number | null {
    const feed = this.oracleFeed.get(asset);
    return feed ? this.clockMs - feed.updatedAtMs : null;
  }

  getPartitions(): PartitionRecord[] {
    return [...this.partitions];
  }

  /** Post-heal consistency: every asset accepted has a finite non-zero price. */
  isConsistent(): boolean {
    for (const [asset, price] of this.acceptedPrices) {
      if (!Number.isFinite(price) || price <= 0) return false;
      const feed = this.oracleFeed.get(asset);
      if (!feed) return false;
    }
    return true;
  }

  private setReachable(name: string, reachable: boolean): void {
    const component = this.components.get(name);
    if (component) component.reachable = reachable;
  }
}

describe('Chaos Engineering: Network Partition Simulation', () => {
  let sim: NetworkPartitionSimulator;

  beforeEach(() => {
    sim = new NetworkPartitionSimulator();
  });

  describe('API ⇄ RPC partition', () => {
    it('isolates the RPC side deterministically', () => {
      expect(sim.canCall('api', 'rpc')).toBe(true);
      sim.isolate('api-rpc');
      expect(sim.canCall('api', 'rpc')).toBe(false);
      expect(sim.isPartitionActive()).toBe(true);
    });

    it('rejects transaction submission while partitioned', () => {
      sim.isolate('api-rpc');
      const result = sim.callEndpoint('submit-transaction');
      expect(result.ok).toBe(false);
      expect(result.error).toMatch(/partition/i);
    });

    it('keeps health reads available for graceful degradation', () => {
      sim.isolate('api-rpc');
      const health = sim.callEndpoint('get-health');
      expect(health.ok).toBe(true);
    });

    it('refuses overlapping partitions', () => {
      sim.isolate('api-rpc');
      expect(() => sim.isolate('oracle-contract')).toThrow(/already active/);
    });
  });

  describe('Oracle ⇄ Contract partition', () => {
    it('blocks price propagation while partitioned', () => {
      sim.isolate('oracle-contract');
      expect(sim.pushPrice('XLM', 0.2)).toBe(false);
    });

    it('rejects stale oracle data after the partition persists', () => {
      // Seed a fresh price, then partition and advance past the staleness window.
      expect(sim.pushPrice('XLM', 0.15)).toBe(true);
      sim.isolate('oracle-contract');
      sim.advanceClock(120_000);

      const decision = sim.contractAcceptsPrice('XLM');
      expect(decision.accepted).toBe(false);
      expect(decision.reason).toMatch(/stale/);
      expect(sim.getAcceptedPrice('XLM')).toBeUndefined();
    });

    it('accepts fresh prices again after heal', () => {
      sim.isolate('oracle-contract');
      sim.advanceClock(120_000);
      sim.heal();

      expect(sim.pushPrice('XLM', 0.18)).toBe(true);
      const decision = sim.contractAcceptsPrice('XLM');
      expect(decision.accepted).toBe(true);
      expect(sim.getAcceptedPrice('XLM')).toBe(0.18);
    });
  });

  describe('Partial endpoint partition', () => {
    it('partitions a subset of endpoints only', () => {
      sim.isolate('partial-endpoint');
      expect(sim.callEndpoint('submit-transaction').ok).toBe(false);
      expect(sim.callEndpoint('get-ledger-entry').ok).toBe(true);
    });

    it('restores all endpoints on heal', () => {
      sim.isolate('partial-endpoint');
      sim.heal();
      for (const endpoint of ['submit-transaction', 'get-ledger-entry', 'simulate-transaction']) {
        expect(sim.callEndpoint(endpoint).ok).toBe(true);
      }
    });
  });

  describe('Split-brain healing and consistency', () => {
    it('records partition lifecycle with heal timestamps', () => {
      sim.isolate('api-rpc');
      sim.advanceClock(5_000);
      const healed = sim.heal();

      expect(healed.startMs).toBe(0);
      expect(healed.healedAtMs).toBe(5_000);
      expect(sim.getPartitions()).toHaveLength(1);
      expect(sim.isPartitionActive()).toBe(false);
    });

    it('heals only when a partition is active', () => {
      expect(() => sim.heal()).toThrow(/no active partition/);
    });

    it('maintains data consistency across a full partition cycle', () => {
      expect(sim.pushPrice('USDC', 1.0)).toBe(true);
      expect(sim.contractAcceptsPrice('USDC').accepted).toBe(true);

      sim.isolate('oracle-contract');
      sim.advanceClock(30_000);
      sim.heal();

      expect(sim.pushPrice('USDC', 1.001)).toBe(true);
      expect(sim.contractAcceptsPrice('USDC').accepted).toBe(true);
      expect(sim.isConsistent()).toBe(true);
    });

    it('supports sequential partitions of different sides', () => {
      sim.isolate('api-rpc');
      sim.heal();
      sim.isolate('oracle-contract');
      expect(sim.canCall('oracle', 'contract')).toBe(false);
      sim.heal();
      expect(sim.canCall('oracle', 'contract')).toBe(true);
      expect(sim.getPartitions()).toHaveLength(2);
    });
  });
});
