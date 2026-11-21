/**
 * Issue #1600 — EvaluatorActor
 *
 * Receives position batches from FeedActor, applies profit/HF filters and the
 * selection strategy, then enqueues individual liquidation jobs onto the
 * ExecutorActor channel.
 *
 * The original `evaluatePositions` / `selectTargets` logic lived inline on
 * `LiquidationBotService`, sharing class-level mutable state with the executor.
 * Moving it here isolates filtering from execution: the evaluator is stateless
 * between batches, so its logic can be unit-tested without touching network
 * code or timers.
 */

import { ActorChannel, FeedMessage, JobMessage, MetricEvent, PositionHealth } from './types';
import { Logger } from '../logger';
import { BotConfig } from '../config';

export class EvaluatorActor {
  private readonly in: ActorChannel<FeedMessage>;
  private readonly out: ActorChannel<JobMessage>;
  private readonly metrics: ActorChannel<MetricEvent>;
  private readonly config: BotConfig;
  private readonly logger: Logger;

  constructor(
    inChannel: ActorChannel<FeedMessage>,
    out: ActorChannel<JobMessage>,
    metrics: ActorChannel<MetricEvent>,
    config: BotConfig,
    logger: Logger,
  ) {
    this.in = inChannel;
    this.out = out;
    this.metrics = metrics;
    this.config = config;
    this.logger = logger;
  }

  /**
   * Run the evaluator loop until the input channel is drained and closed.
   *
   * Returns a Promise that resolves when the actor has finished processing all
   * buffered messages and forwarded its own shutdown signal downstream.
   */
  async run(): Promise<void> {
    this.logger.info('[EvaluatorActor] started');
    for (;;) {
      const msg = await this.in.recv();
      if (msg === null || msg.kind === 'shutdown') {
        // Propagate shutdown to executor.
        await this.out.send({ kind: 'shutdown' });
        this.out.close();
        this.logger.info('[EvaluatorActor] stopped');
        return;
      }
      await this.evaluate(msg.positions);
    }
  }

  // ── Filtering and selection ───────────────────────────────────────────────

  private async evaluate(positions: PositionHealth[]): Promise<void> {
    const liquidatable = positions.filter((p) => {
      const profitXlm = p.netProfit / 10_000_000;
      return p.healthFactor < 1.2 && profitXlm >= this.config.minProfitThresholdXlm;
    });

    if (liquidatable.length === 0) return;

    const selected = this.selectTargets(liquidatable);
    for (const target of selected) {
      await this.out.send({ kind: 'liquidate', position: target });
    }
  }

  private selectTargets(positions: PositionHealth[]): PositionHealth[] {
    const sorted = [...positions];
    switch (this.config.liquidationTargetStrategy) {
      case 'highest_profit':
        sorted.sort((a, b) => b.netProfit - a.netProfit);
        break;
      case 'highest_risk':
      case 'lowest_hf':
        sorted.sort((a, b) => a.healthFactor - b.healthFactor);
        break;
    }
    return sorted.slice(0, this.config.maxConcurrentLiquidations);
  }
}
