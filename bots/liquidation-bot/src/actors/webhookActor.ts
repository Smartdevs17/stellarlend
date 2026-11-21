/**
 * Issue #1600 — WebhookActor
 *
 * Receives confirmed liquidation results from the ExecutorActor and delivers
 * them via the existing `LiquidationWebhookService`.  Separating webhook
 * delivery into its own actor means a slow or failing webhook endpoint cannot
 * block the executor from processing the next job.
 */

import { LiquidationWebhookService, ConfirmedLiquidation } from '../liquidationWebhook.service';
import { ActorChannel, MetricEvent, WebhookMessage } from './types';
import { Logger } from '../logger';

export class WebhookActor {
  private readonly in: ActorChannel<WebhookMessage>;
  private readonly metrics: ActorChannel<MetricEvent>;
  private readonly webhook: LiquidationWebhookService;
  private readonly logger: Logger;

  constructor(
    inChannel: ActorChannel<WebhookMessage>,
    metrics: ActorChannel<MetricEvent>,
    webhook: LiquidationWebhookService,
    logger: Logger,
  ) {
    this.in = inChannel;
    this.metrics = metrics;
    this.webhook = webhook;
    this.logger = logger;
  }

  async run(): Promise<void> {
    this.logger.info('[WebhookActor] started');
    for (;;) {
      const msg = await this.in.recv();
      if (msg === null || msg.kind === 'shutdown') {
        await this.metrics.send({ kind: 'shutdown' });
        this.metrics.close();
        this.logger.info('[WebhookActor] stopped');
        return;
      }

      const { result } = msg;
      if (!result.success || !result.txHash || result.profitStroops === undefined) {
        continue;
      }

      const confirmed: ConfirmedLiquidation = {
        positionAddress: result.positionAddress,
        txHash: result.txHash,
        profitStroops: result.profitStroops,
        timestamp: result.timestamp,
      };

      const delivered = await this.webhook.deliver(confirmed);
      if (delivered) {
        await this.metrics.send({
          kind: 'webhook_delivered',
          positionAddress: result.positionAddress,
        });
      } else {
        await this.metrics.send({
          kind: 'webhook_failed',
          positionAddress: result.positionAddress,
        });
        this.logger.warn('[WebhookActor] delivery failed', {
          position: result.positionAddress,
        });
      }
    }
  }
}
