/**
 * Issue #1600 — Async actor types and channel contracts
 *
 * Every actor in the liquidation bot receives messages through a typed channel.
 * This file defines the message envelopes so the compiler catches mismatches at
 * the boundary between actors rather than at runtime.
 *
 * ## Actor topology
 *
 *   FeedActor  ──(PositionHealth[])──►  EvaluatorActor
 *                                              │
 *                                   (LiquidationJob)
 *                                              │
 *                                              ▼
 *   MetricsActor ◄──(MetricEvent)── ExecutorActor ──► WebhookActor
 *
 * Each arrow is a typed `ActorChannel<T>`.  Actors never share mutable state;
 * they communicate only through these channels.
 */

// ── Shared primitives ─────────────────────────────────────────────────────────

export interface PositionHealth {
  address: string;
  collateralAsset: string;
  debtAsset: string;
  collateralValue: number;
  debtValue: number;
  healthFactor: number;
  estimatedProfit: number;
  gasCostStroops: number;
  netProfit: number;
}

export interface LiquidationResult {
  positionAddress: string;
  success: boolean;
  txHash?: string;
  profitStroops?: number;
  error?: string;
  timestamp: number;
  latencyMs: number;
}

// ── Channel message types ─────────────────────────────────────────────────────

/** Messages sent from FeedActor → EvaluatorActor */
export type FeedMessage =
  | { kind: 'positions'; positions: PositionHealth[] }
  | { kind: 'shutdown' };

/** Messages sent from EvaluatorActor → ExecutorActor */
export type JobMessage =
  | { kind: 'liquidate'; position: PositionHealth }
  | { kind: 'shutdown' };

/** Messages sent from ExecutorActor → WebhookActor */
export type WebhookMessage =
  | { kind: 'confirmed'; result: LiquidationResult }
  | { kind: 'shutdown' };

/** Messages sent from any actor → MetricsActor */
export type MetricEvent =
  | { kind: 'attempt'; positionAddress: string }
  | { kind: 'success'; positionAddress: string; profitStroops: number; latencyMs: number }
  | { kind: 'failure'; positionAddress: string; error: string; latencyMs: number }
  | { kind: 'skipped_inflight'; positionAddress: string }
  | { kind: 'skipped_breaker'; positionAddress: string }
  | { kind: 'webhook_delivered'; positionAddress: string }
  | { kind: 'webhook_failed'; positionAddress: string }
  | { kind: 'active_count'; count: number }
  | { kind: 'breaker_open_count'; count: number }
  | { kind: 'shutdown' };

// ── Typed channel ─────────────────────────────────────────────────────────────

/**
 * A bounded, back-pressure-providing async channel between two actors.
 *
 * Internally backed by a fixed-length queue; `send` resolves immediately while
 * the queue has capacity and awaits a drain otherwise.  This propagates back-
 * pressure up to the producer rather than growing memory without bound.
 */
export interface ActorChannel<T> {
  /** Send a message, awaiting capacity if the queue is full. */
  send(msg: T): Promise<void>;
  /**
   * Receive the next message, awaiting one if the queue is empty.
   * Returns `null` when the channel has been closed and drained.
   */
  recv(): Promise<T | null>;
  /** Close the channel; subsequent `recv` calls drain then return `null`. */
  close(): void;
  /** Number of messages currently buffered. */
  readonly size: number;
}

/** Create a bounded channel with the given capacity. */
export function createChannel<T>(capacity = 128): ActorChannel<T> {
  const queue: T[] = [];
  let closed = false;

  // Pending resolvers waiting for a message (recv side)
  const recvWaiters: Array<(msg: T | null) => void> = [];
  // Pending resolvers waiting for capacity (send side)
  const sendWaiters: Array<() => void> = [];

  return {
    get size() {
      return queue.length;
    },

    async send(msg: T): Promise<void> {
      if (closed) {
        throw new Error('Cannot send to a closed channel');
      }
      // If a recv is already waiting, hand it the message directly.
      if (recvWaiters.length > 0) {
        const resolve = recvWaiters.shift()!;
        resolve(msg);
        return;
      }
      // Queue has capacity — push immediately.
      if (queue.length < capacity) {
        queue.push(msg);
        return;
      }
      // Back-pressure: wait until there is room.
      await new Promise<void>((resolve) => sendWaiters.push(resolve));
      queue.push(msg);
    },

    async recv(): Promise<T | null> {
      if (queue.length > 0) {
        const msg = queue.shift()!;
        // Unblock a waiting sender if any.
        if (sendWaiters.length > 0) {
          const resolve = sendWaiters.shift()!;
          resolve();
        }
        return msg;
      }
      if (closed) {
        return null;
      }
      return new Promise<T | null>((resolve) => {
        recvWaiters.push(resolve);
      });
    },

    close() {
      closed = true;
      // Drain all waiting receivers with null.
      while (recvWaiters.length > 0) {
        const resolve = recvWaiters.shift()!;
        resolve(null);
      }
    },
  };
}
