/**
 * Issue #1600 — ExecutorActor
 *
 * Owns all mutable execution state: the in-flight set, the circuit-breaker
 * registry, and the active-liquidation counter.  It is the only actor that
 * touches those resources, eliminating the race conditions that existed when
 * both the WebSocket handler and the poll loop could call `executeLiquidation`
 * concurrently on the original service class.
 *
 * Concurrency is capped by `config.maxConcurrentLiquidations`: the actor
 * spawns up to that many concurrent `executeLiquidation` Promises and gates
 * new jobs behind the capacity check without blocking the message loop.
 */

import axios from 'axios';
import { ActorChannel, JobMessage, LiquidationResult, MetricEvent, WebhookMessage } from './types';
import { CircuitBreakerRegistry } from '../circuitBreaker';
import { Logger } from '../logger';
import { BotConfig } from '../config';

export class ExecutorActor {
  private readonly in: ActorChannel<JobMessage>;
  private readonly webhook: ActorChannel<WebhookMessage>;
  private readonly metrics: ActorChannel<MetricEvent>;
  private readonly config: BotConfig;
  private readonly logger: Logger;

  private readonly inFlight = new Set<string>();
  private readonly breaker: CircuitBreakerRegistry;
  private activeLiquidations = 0;

  constructor(
    inChannel: ActorChannel<JobMessage>,
    webhook: ActorChannel<WebhookMessage>,
    metrics: ActorChannel<MetricEvent>,
    config: BotConfig,
    logger: Logger,
    breaker: CircuitBreakerRegistry = new CircuitBreakerRegistry(),
  ) {
    this.in = inChannel;
    this.webhook = webhook;
    this.metrics = metrics;
    this.config = config;
    this.logger = logger;
    this.breaker = breaker;
  }

  /**
   * Run until the input channel is drained and closed.
   *
   * The message loop itself is single-threaded: it dequeues one job at a time
   * and fires `void executeLiquidation(...)` without awaiting it, so the loop
   * never blocks on network I/O.  The concurrency cap is enforced by checking
   * `activeLiquidations` before spawning.
   */
  async run(): Promise<void> {
    this.logger.info('[ExecutorActor] started');
    for (;;) {
      const msg = await this.in.recv();
      if (msg === null || msg.kind === 'shutdown') {
        // Propagate shutdown; wait for in-flight liquidations to settle.
        await this.drainInflight();
        await this.webhook.send({ kind: 'shutdown' });
        this.webhook.close();
        this.logger.info('[ExecutorActor] stopped');
        return;
      }

      const { position } = msg;

      // ── Concurrency cap ───────────────────────────────────────────────────
      if (this.activeLiquidations >= this.config.maxConcurrentLiquidations) {
        this.logger.debug('[ExecutorActor] concurrency cap reached; dropping job', {
          position: position.address,
          active: this.activeLiquidations,
        });
        continue;
      }

      // ── In-flight deduplication ───────────────────────────────────────────
      if (this.inFlight.has(position.address)) {
        await this.metrics.send({ kind: 'skipped_inflight', positionAddress: position.address });
        continue;
      }

      // ── Circuit breaker ───────────────────────────────────────────────────
      if (!this.breaker.isAllowed(position.address)) {
        await this.metrics.send({
          kind: 'skipped_breaker',
          positionAddress: position.address,
        });
        continue;
      }

      // Fire and forget — the loop continues immediately.
      void this.executeLiquidation(position);
    }
  }

  // ── Private execution helpers ─────────────────────────────────────────────

  private async executeLiquidation(position: {
    address: string;
    debtValue: number;
    estimatedProfit: number;
    gasCostStroops: number;
    netProfit: number;
    healthFactor: number;
  }): Promise<void> {
    this.inFlight.add(position.address);
    this.activeLiquidations++;
    await this.metrics.send({ kind: 'active_count', count: this.activeLiquidations });
    await this.metrics.send({ kind: 'attempt', positionAddress: position.address });

    const startedAt = Date.now();
    const profitXlm = position.netProfit / 10_000_000;

    this.logger.info('[ExecutorActor] attempting liquidation', {
      position: position.address,
      profitXlm: profitXlm.toFixed(2),
      hf: position.healthFactor,
    });

    if (this.config.dryRun) {
      this.logger.info('[ExecutorActor] DRY RUN — would liquidate', {
        position: position.address,
      });
      this.breaker.recordSuccess(position.address);
      const result: LiquidationResult = {
        positionAddress: position.address,
        success: true,
        profitStroops: position.estimatedProfit,
        timestamp: Date.now(),
        latencyMs: Date.now() - startedAt,
      };
      await this.metrics.send({
        kind: 'success',
        positionAddress: position.address,
        profitStroops: position.estimatedProfit,
        latencyMs: result.latencyMs,
      });
      await this.webhook.send({ kind: 'confirmed', result });
      this.finalise(position.address, startedAt);
      return;
    }

    try {
      const res = await axios.post(
        `${this.config.apiBaseUrl}/lending/liquidate`,
        {
          positionAddress: position.address,
          maxRepayAmount: Math.floor(position.debtValue * 0.5).toString(),
        },
        { timeout: 30_000 },
      );

      const latencyMs = Date.now() - startedAt;
      const result: LiquidationResult = {
        positionAddress: position.address,
        success: true,
        txHash: res.data?.txHash,
        profitStroops: position.estimatedProfit,
        timestamp: Date.now(),
        latencyMs,
      };

      this.breaker.recordSuccess(position.address);
      await this.metrics.send({
        kind: 'success',
        positionAddress: position.address,
        profitStroops: position.estimatedProfit,
        latencyMs,
      });
      await this.webhook.send({ kind: 'confirmed', result });

      this.logger.info('[ExecutorActor] liquidation successful', {
        position: position.address,
        txHash: result.txHash,
        latencyMs,
      });
    } catch (err: any) {
      const latencyMs = Date.now() - startedAt;
      this.breaker.recordFailure(position.address);
      await this.metrics.send({
        kind: 'failure',
        positionAddress: position.address,
        error: err.message ?? 'unknown',
        latencyMs,
      });
      this.logger.error('[ExecutorActor] liquidation failed', {
        position: position.address,
        error: err.message,
        breakerState: this.breaker.getState(position.address),
      });
    } finally {
      this.finalise(position.address, startedAt);
    }
  }

  private finalise(address: string, startedAt: number): void {
    this.inFlight.delete(address);
    this.activeLiquidations--;
    void this.metrics.send({ kind: 'active_count', count: this.activeLiquidations });
    void this.metrics.send({
      kind: 'breaker_open_count',
      count: this.breaker.openCount(),
    });
    this.logger.debug('[ExecutorActor] finalised', {
      position: address,
      latencyMs: Date.now() - startedAt,
    });
  }

  /** Wait until all in-flight liquidations complete before propagating shutdown. */
  private async drainInflight(): Promise<void> {
    if (this.inFlight.size === 0) return;
    this.logger.info('[ExecutorActor] draining in-flight liquidations', {
      count: this.inFlight.size,
    });
    // Poll at 100ms intervals; in-flight promises call finalise which deletes
    // entries from the set, so this loop will eventually see size === 0.
    await new Promise<void>((resolve) => {
      const check = () => {
        if (this.inFlight.size === 0) {
          resolve();
        } else {
          setTimeout(check, 100);
        }
      };
      check();
    });
  }

  /** Expose the breaker for inspection / testing. */
  getBreakerRegistry(): CircuitBreakerRegistry {
    return this.breaker;
  }
}
