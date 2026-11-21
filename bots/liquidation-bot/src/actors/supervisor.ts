/**
 * Issue #1600 — ActorSupervisor
 *
 * Replaces `LiquidationBotService` as the top-level orchestrator.  Instead of
 * a single class with shared mutable state, it creates five actors connected by
 * typed channels, starts their run-loops as independent async tasks, and
 * provides a clean shutdown path that propagates through the pipeline in order.
 *
 * ## Lifecycle
 *
 *   supervisor.start()
 *     → FeedActor.start()          (spawns WS + poll timer)
 *     → EvaluatorActor.run()       (runs until FeedActor sends shutdown)
 *     → ExecutorActor.run()        (runs until EvaluatorActor sends shutdown)
 *     → WebhookActor.run()         (runs until ExecutorActor sends shutdown)
 *     → MetricsActor.run()         (runs until WebhookActor sends shutdown)
 *
 *   supervisor.stop()
 *     → FeedActor.stop()           (sends shutdown on feedChannel)
 *     → awaits all actor Promises  (each actor drains then propagates shutdown)
 */

import { botConfig, BotConfig } from '../config';
import { Logger } from '../logger';
import { LiquidationWebhookService } from '../liquidationWebhook.service';
import { CircuitBreakerRegistry } from '../circuitBreaker';

import { createChannel, FeedMessage, JobMessage, MetricEvent, WebhookMessage } from './types';
import { FeedActor } from './feedActor';
import { EvaluatorActor } from './evaluatorActor';
import { ExecutorActor } from './executorActor';
import { WebhookActor } from './webhookActor';
import { MetricsActor } from './metricsActor';

export class ActorSupervisor {
  private readonly config: BotConfig;
  private readonly logger: Logger;

  private feed!: FeedActor;
  private executor!: ExecutorActor;

  private readonly actorTasks: Promise<void>[] = [];

  constructor(
    config: BotConfig = botConfig,
    logger: Logger = new Logger(botConfig.logLevel),
  ) {
    this.config = config;
    this.logger = logger;
  }

  async start(): Promise<void> {
    this.logger.info('[ActorSupervisor] starting', {
      strategy: this.config.liquidationTargetStrategy,
      dryRun: this.config.dryRun,
    });

    // ── Build channels ────────────────────────────────────────────────────────
    const feedChannel   = createChannel<FeedMessage>(256);
    const jobChannel    = createChannel<JobMessage>(256);
    const webhookChannel = createChannel<WebhookMessage>(128);
    const metricsChannel = createChannel<MetricEvent>(512);

    // ── Instantiate actors ────────────────────────────────────────────────────
    this.feed = new FeedActor(feedChannel, this.config, this.logger);

    const evaluator = new EvaluatorActor(
      feedChannel,
      jobChannel,
      metricsChannel,
      this.config,
      this.logger,
    );

    const webhookService = new LiquidationWebhookService(
      {
        url: this.config.liquidationWebhookUrl,
        secret: this.config.liquidationWebhookSecret,
        maxAttempts: this.config.webhookMaxAttempts,
        retryDelayMs: this.config.webhookRetryDelayMs,
      },
      this.logger,
    );

    this.executor = new ExecutorActor(
      jobChannel,
      webhookChannel,
      metricsChannel,
      this.config,
      this.logger,
      new CircuitBreakerRegistry(),
    );

    const webhookActor = new WebhookActor(
      webhookChannel,
      metricsChannel,
      webhookService,
      this.logger,
    );

    const metricsActor = new MetricsActor(metricsChannel, this.logger);

    // ── Start run-loops ───────────────────────────────────────────────────────
    // FeedActor drives itself with timers; the rest run as async loops.
    this.feed.start();

    this.actorTasks.push(
      evaluator.run(),
      this.executor.run(),
      webhookActor.run(),
      metricsActor.run(),
    );

    this.logger.info('[ActorSupervisor] all actors running');
  }

  async stop(): Promise<void> {
    this.logger.info('[ActorSupervisor] stopping');
    await this.feed.stop();          // sends shutdown → feedChannel
    await Promise.all(this.actorTasks); // wait for the full pipeline to drain
    this.logger.info('[ActorSupervisor] all actors stopped');
  }

  /** Expose the circuit-breaker for operator tooling / tests. */
  getBreakerRegistry(): CircuitBreakerRegistry {
    return this.executor.getBreakerRegistry();
  }
}
