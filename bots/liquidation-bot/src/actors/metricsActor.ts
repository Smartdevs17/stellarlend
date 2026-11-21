/**
 * Issue #1600 — MetricsActor
 *
 * Single writer for all protocol counters and gauges.  Every other actor sends
 * `MetricEvent` messages here rather than mutating a shared `botMetrics`
 * object, removing the data-race risk that existed when multiple async branches
 * incremented counters directly.
 */

import { botMetrics } from '../metrics';
import { ActorChannel, MetricEvent } from './types';
import { Logger } from '../logger';

export class MetricsActor {
  private readonly in: ActorChannel<MetricEvent>;
  private readonly logger: Logger;

  constructor(inChannel: ActorChannel<MetricEvent>, logger: Logger) {
    this.in = inChannel;
    this.logger = logger;
  }

  async run(): Promise<void> {
    this.logger.info('[MetricsActor] started');
    for (;;) {
      const msg = await this.in.recv();
      if (msg === null || msg.kind === 'shutdown') {
        this.logger.info('[MetricsActor] stopped');
        return;
      }
      this.apply(msg);
    }
  }

  private apply(event: MetricEvent): void {
    switch (event.kind) {
      case 'attempt':
        botMetrics.liquidation_attempts_total++;
        break;
      case 'success':
        botMetrics.liquidation_successes_total++;
        botMetrics.recordLiquidationLatency(event.latencyMs);
        break;
      case 'failure':
        botMetrics.liquidation_failures_total++;
        botMetrics.recordLiquidationLatency(event.latencyMs);
        break;
      case 'skipped_inflight':
        botMetrics.liquidation_skipped_inflight_total++;
        break;
      case 'skipped_breaker':
        botMetrics.liquidation_skipped_circuitbreaker_total++;
        break;
      case 'webhook_delivered':
        botMetrics.webhook_deliveries_total++;
        break;
      case 'webhook_failed':
        botMetrics.webhook_failures_total++;
        break;
      case 'active_count':
        botMetrics.active_liquidations = event.count;
        break;
      case 'breaker_open_count':
        botMetrics.circuit_breaker_open_count = event.count;
        break;
      case 'shutdown':
        // handled in run()
        break;
    }
  }
}
