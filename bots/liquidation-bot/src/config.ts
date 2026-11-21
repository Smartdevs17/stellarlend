import dotenv from 'dotenv';
dotenv.config();

// Issue #1602: shared network + bot fields come from @stellarlend/config.
// Bot-specific fields (minProfitThresholdXlm, etc.) are added below.
import {
  BotSharedConfig,
  StellarNetworkConfig,
  loadBotSharedConfig,
  loadStellarConfig,
  envInt,
  envBool,
} from '../../../packages/config/src/index';

/** Full config for the liquidation bot. */
export interface BotConfig extends BotSharedConfig {
  // ── Stellar connection (from shared package) ──────────────────────────────
  stellarNetwork: StellarNetworkConfig['network'];
  stellarRpcUrl: string;
  stellarHorizonUrl: string;
  networkPassphrase: string;

  // ── Signing keys ──────────────────────────────────────────────────────────
  botSecretKey: string;
  botPublicKey: string;

  // ── Liquidation bot-specific ──────────────────────────────────────────────
  minProfitThresholdXlm: number;
  maxGasPriceStroops: number;
  liquidationTargetStrategy: 'highest_profit' | 'highest_risk' | 'lowest_hf';
  pollIntervalMs: number;
  maxConcurrentLiquidations: number;

  // ── Webhook ───────────────────────────────────────────────────────────────
  liquidationWebhookUrl: string;
  liquidationWebhookSecret: string;
  webhookMaxAttempts: number;
  webhookRetryDelayMs: number;

  /**
   * Minimum discount (in basis points) required before the auction bidder
   * considers placing a bid.  Default: 500 bps (5%).
   */
  minDiscountBps: number;
}

export function loadConfig(): BotConfig {
  const stellar = loadStellarConfig();
  const shared  = loadBotSharedConfig();

  return {
    // Shared fields
    ...shared,

    // Stellar connection
    stellarNetwork:    stellar.network,
    stellarRpcUrl:     stellar.rpcUrl,
    stellarHorizonUrl: stellar.horizonUrl,
    networkPassphrase: stellar.networkPassphrase,

    // Signing keys
    botSecretKey: process.env.BOT_SECRET_KEY ?? '',
    botPublicKey:  process.env.BOT_PUBLIC_KEY  ?? '',

    // Liquidation bot-specific
    minProfitThresholdXlm:    parseFloat(process.env.MIN_PROFIT_THRESHOLD_XLM  ?? '1.0'),
    maxGasPriceStroops:       envInt('MAX_GAS_PRICE_STROOPS', 100_000),
    liquidationTargetStrategy: (process.env.LIQUIDATION_TARGET_STRATEGY ?? 'highest_profit') as BotConfig['liquidationTargetStrategy'],
    pollIntervalMs:            envInt('POLL_INTERVAL_MS', 5_000),
    maxConcurrentLiquidations: envInt('MAX_CONCURRENT_LIQUIDATIONS', 3),

    // Webhook
    liquidationWebhookUrl:    process.env.LIQUIDATION_WEBHOOK_URL    ?? '',
    liquidationWebhookSecret: process.env.LIQUIDATION_WEBHOOK_SECRET ?? '',
    webhookMaxAttempts:       envInt('WEBHOOK_MAX_ATTEMPTS', 3),
    webhookRetryDelayMs:      envInt('WEBHOOK_RETRY_DELAY_MS', 500),

    minDiscountBps: envInt('MIN_DISCOUNT_BPS', 500),
  };
}

export const botConfig = loadConfig();
