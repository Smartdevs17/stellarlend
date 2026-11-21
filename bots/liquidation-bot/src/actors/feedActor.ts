/**
 * Issue #1600 — FeedActor
 *
 * Owns the WebSocket connection and the HTTP poll loop.  Both event sources
 * write `PositionHealth[]` batches to the same `ActorChannel<FeedMessage>`;
 * the EvaluatorActor consumes them without caring about the source.
 *
 * Structuring the two sources as one actor eliminates the shared-state problem
 * in the original `LiquidationBotService`, where `connectWebSocket` and
 * `startPolling` both called `evaluatePositions` directly — a function that
 * mutated `activeLiquidations` and `inFlight` from multiple concurrent call
 * stacks.
 */

import WebSocket from 'ws';
import axios from 'axios';
import { ActorChannel, FeedMessage, PositionHealth } from './types';
import { Logger } from '../logger';
import { BotConfig } from '../config';

export class FeedActor {
  private readonly out: ActorChannel<FeedMessage>;
  private readonly config: BotConfig;
  private readonly logger: Logger;

  private ws: WebSocket | null = null;
  private running = false;
  private pollTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    out: ActorChannel<FeedMessage>,
    config: BotConfig,
    logger: Logger,
  ) {
    this.out = out;
    this.config = config;
    this.logger = logger;
  }

  /** Start the WebSocket and poll loop, returning immediately. */
  start(): void {
    this.running = true;
    this.connectWebSocket();
    this.schedulePoll();
  }

  /** Signal graceful shutdown; drains any in-flight timer then closes. */
  async stop(): Promise<void> {
    this.running = false;
    if (this.pollTimer !== null) {
      clearTimeout(this.pollTimer);
      this.pollTimer = null;
    }
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    await this.out.send({ kind: 'shutdown' });
    this.out.close();
    this.logger.info('[FeedActor] stopped');
  }

  // ── WebSocket ─────────────────────────────────────────────────────────────

  private connectWebSocket(): void {
    try {
      this.ws = new WebSocket(this.config.wsUrl);

      this.ws.on('open', () => {
        this.logger.info('[FeedActor] WebSocket connected');
      });

      this.ws.on('message', async (data: string) => {
        try {
          const msg = JSON.parse(data);
          if (msg.type === 'health_update' || msg.type === 'health_snapshot') {
            const positions: PositionHealth[] = msg.positions ?? [];
            if (positions.length > 0) {
              await this.out.send({ kind: 'positions', positions });
            }
          }
        } catch (err) {
          this.logger.error('[FeedActor] failed to parse WS message', { error: err });
        }
      });

      this.ws.on('close', () => {
        this.logger.warn('[FeedActor] WebSocket disconnected; reconnecting in 5s');
        if (this.running) {
          setTimeout(() => this.connectWebSocket(), 5_000);
        }
      });

      this.ws.on('error', (err: Error) => {
        this.logger.error('[FeedActor] WebSocket error', { error: err.message });
      });
    } catch (err) {
      this.logger.error('[FeedActor] failed to open WebSocket', { error: err });
    }
  }

  // ── HTTP poll loop ────────────────────────────────────────────────────────

  private schedulePoll(): void {
    if (!this.running) return;
    this.pollTimer = setTimeout(() => this.poll(), this.config.pollIntervalMs);
  }

  private async poll(): Promise<void> {
    if (!this.running) return;
    try {
      const res = await axios.get(
        `${this.config.apiBaseUrl}/liquidations/opportunities`,
        {
          params: { maxHf: 1.2, sortBy: 'netProfitStroops', sortDir: 'desc' },
          timeout: 10_000,
        },
      );
      const positions: PositionHealth[] = res.data?.opportunities ?? [];
      if (positions.length > 0) {
        await this.out.send({ kind: 'positions', positions });
      }
    } catch (err: any) {
      this.logger.warn('[FeedActor] poll failed', { error: err.message });
    }
    this.schedulePoll();
  }
}
