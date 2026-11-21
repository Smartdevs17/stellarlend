/**
 * Issue #1600 — actor tests
 *
 * Tests the channel primitive and each actor in isolation without network I/O.
 */

import { createChannel, FeedMessage, JobMessage, MetricEvent } from '../types';
import { EvaluatorActor } from '../evaluatorActor';
import { BotConfig } from '../../config';
import { Logger } from '../../logger';

// ── Minimal stubs ─────────────────────────────────────────────────────────────

function makeConfig(overrides: Partial<BotConfig> = {}): BotConfig {
  return {
    stellarNetwork: 'testnet',
    stellarRpcUrl: '',
    stellarHorizonUrl: '',
    botSecretKey: '',
    botPublicKey: '',
    minProfitThresholdXlm: 1.0,
    maxGasPriceStroops: 100_000,
    liquidationTargetStrategy: 'highest_profit',
    pollIntervalMs: 5_000,
    dryRun: true,
    maxConcurrentLiquidations: 3,
    wsUrl: '',
    apiBaseUrl: '',
    liquidationWebhookUrl: '',
    liquidationWebhookSecret: '',
    webhookMaxAttempts: 3,
    webhookRetryDelayMs: 500,
    logLevel: 'silent',
    minDiscountBps: 500,
    ...overrides,
  };
}

const silentLogger = new Logger('silent');

function makePosition(overrides: Record<string, unknown> = {}) {
  return {
    address: 'ADDR1',
    collateralAsset: 'XLM',
    debtAsset: 'USDC',
    collateralValue: 200,
    debtValue: 100,
    healthFactor: 0.9,
    estimatedProfit: 5_000_000,
    gasCostStroops: 1_000_000,
    netProfit: 4_000_000,  // 0.4 XLM > minProfitThresholdXlm 1.0? No — use 15_000_000
    ...overrides,
  };
}

// ── Channel tests ─────────────────────────────────────────────────────────────

describe('createChannel', () => {
  it('sends and receives a single message', async () => {
    const ch = createChannel<number>(4);
    await ch.send(42);
    const received = await ch.recv();
    expect(received).toBe(42);
  });

  it('receives null after close with empty queue', async () => {
    const ch = createChannel<number>(4);
    ch.close();
    const received = await ch.recv();
    expect(received).toBeNull();
  });

  it('drains buffered messages before returning null on close', async () => {
    const ch = createChannel<number>(4);
    await ch.send(1);
    await ch.send(2);
    ch.close();
    expect(await ch.recv()).toBe(1);
    expect(await ch.recv()).toBe(2);
    expect(await ch.recv()).toBeNull();
  });

  it('reports correct size', async () => {
    const ch = createChannel<string>(8);
    expect(ch.size).toBe(0);
    await ch.send('a');
    await ch.send('b');
    expect(ch.size).toBe(2);
    await ch.recv();
    expect(ch.size).toBe(1);
  });

  it('throws when sending to a closed channel', async () => {
    const ch = createChannel<number>(4);
    ch.close();
    await expect(ch.send(1)).rejects.toThrow('closed');
  });
});

// ── EvaluatorActor tests ──────────────────────────────────────────────────────

describe('EvaluatorActor', () => {
  it('filters out positions below the profit threshold', async () => {
    const feedCh = createChannel<FeedMessage>(8);
    const jobCh  = createChannel<JobMessage>(8);
    const metCh  = createChannel<MetricEvent>(8);

    const cfg = makeConfig({ minProfitThresholdXlm: 2.0 });
    const actor = new EvaluatorActor(feedCh, jobCh, metCh, cfg, silentLogger);

    const runPromise = actor.run();

    // Position with netProfit = 10_000_000 stroops = 1.0 XLM < 2.0 threshold
    await feedCh.send({
      kind: 'positions',
      positions: [makePosition({ netProfit: 10_000_000 })],
    });
    await feedCh.send({ kind: 'shutdown' });

    await runPromise;

    // jobCh should have only received the shutdown forwarded; no liquidate job.
    const msg = await jobCh.recv();
    expect(msg?.kind).toBe('shutdown');
  });

  it('forwards liquidatable positions as jobs', async () => {
    const feedCh = createChannel<FeedMessage>(8);
    const jobCh  = createChannel<JobMessage>(8);
    const metCh  = createChannel<MetricEvent>(8);

    const cfg = makeConfig({ minProfitThresholdXlm: 1.0 });
    const actor = new EvaluatorActor(feedCh, jobCh, metCh, cfg, silentLogger);

    const runPromise = actor.run();

    // netProfit = 20_000_000 stroops = 2.0 XLM > threshold, hf = 0.8 < 1.2
    await feedCh.send({
      kind: 'positions',
      positions: [makePosition({ netProfit: 20_000_000, healthFactor: 0.8 })],
    });
    await feedCh.send({ kind: 'shutdown' });

    await runPromise;

    const job = await jobCh.recv();
    expect(job?.kind).toBe('liquidate');
    if (job?.kind === 'liquidate') {
      expect(job.position.address).toBe('ADDR1');
    }
  });

  it('sorts by highest_profit strategy', async () => {
    const feedCh = createChannel<FeedMessage>(16);
    const jobCh  = createChannel<JobMessage>(16);
    const metCh  = createChannel<MetricEvent>(8);

    const cfg = makeConfig({
      minProfitThresholdXlm: 1.0,
      liquidationTargetStrategy: 'highest_profit',
      maxConcurrentLiquidations: 2,
    });
    const actor = new EvaluatorActor(feedCh, jobCh, metCh, cfg, silentLogger);
    const runPromise = actor.run();

    await feedCh.send({
      kind: 'positions',
      positions: [
        makePosition({ address: 'LOW',  netProfit: 20_000_000, healthFactor: 0.9 }),
        makePosition({ address: 'HIGH', netProfit: 50_000_000, healthFactor: 0.95 }),
      ],
    });
    await feedCh.send({ kind: 'shutdown' });
    await runPromise;

    const first = await jobCh.recv();
    expect(first?.kind === 'liquidate' && first.position.address).toBe('HIGH');
    const second = await jobCh.recv();
    expect(second?.kind === 'liquidate' && second.position.address).toBe('LOW');
  });

  it('propagates shutdown downstream', async () => {
    const feedCh = createChannel<FeedMessage>(4);
    const jobCh  = createChannel<JobMessage>(4);
    const metCh  = createChannel<MetricEvent>(4);

    const actor = new EvaluatorActor(feedCh, jobCh, metCh, makeConfig(), silentLogger);
    const runPromise = actor.run();

    await feedCh.send({ kind: 'shutdown' });
    await runPromise;

    const msg = await jobCh.recv();
    expect(msg?.kind).toBe('shutdown');
  });
});
