import type { Anomaly, LiquidationMetrics, TransactionAnomaly, TransactionRecord } from '../types.js';

/**
 * Flag unusual liquidations using robust z-scores on discount and net profit.
 *
 * Uses the median and median absolute deviation instead of the mean and
 * standard deviation: liquidation data is heavily right-skewed (a handful of
 * whale losses drag the mean up), so the Gaussian z-score on the mean either
 * blinds us to real outliers or flags every quiet day. Median/MAD keeps the
 * baseline stable and makes the detector robust for skewed populations.
 */
export function detectAnomalies(
  metrics: LiquidationMetrics[],
  zThreshold: number = 3
): Anomaly[] {
  if (metrics.length < 5) return [];

  const discounts = metrics.map((m) => m.discount);
  const profits = metrics.map((m) => m.netProfit);
  const dMed = median(discounts);
  const dMad = mad(discounts, dMed) || 1;
  const pMed = median(profits);
  const pMad = mad(profits, pMed) || 1;

  const anomalies: Anomaly[] = [];
  for (const m of metrics) {
    const dZ = Math.abs((m.discount - dMed) / (1.4826 * dMad));
    const pZ = Math.abs((m.netProfit - pMed) / (1.4826 * pMad));
    const score = Math.max(dZ, pZ);
    if (score >= zThreshold) {
      anomalies.push({
        txHash: m.txHash,
        reason: dZ >= pZ ? 'unusual_discount' : 'unusual_profit',
        score,
        metrics: m,
      });
    }
  }
  return anomalies.sort((a, b) => b.score - a.score);
}

/**
 * Detect unusual transaction patterns on an arbitrary stream of records:
 *
 * - `velocity_burst`   — the sender fires an abnormal number of transactions in
 *   a short window (typical wash-trading / sybil tactics).
 * - `amount_spike`     — a transfer far above the account's own historical size
 *   (structuring attempts or a compromised key dumping funds).
 * - `dusting`          — many tiny transfers to the same recipient, a pattern
 *   used to poison wallets ahead of a follow-up attack.
 *
 * Windows are bucketed by record index relative to the observation count, so
 * callers only need to order the records chronologically.
 */
export function detectTransactionAnomalies(
  txs: TransactionRecord[],
  opts: {
    velocityWindow?: number;
    velocityThreshold?: number;
    spikeThreshold?: number;
    dustMaxAmount?: number;
    dustMinCount?: number;
  } = {}
): TransactionAnomaly[] {
  const velocityWindow = opts.velocityWindow ?? 5;
  const velocityThreshold = opts.velocityThreshold ?? 3;
  const spikeThreshold = opts.spikeThreshold ?? 4;
  const dustMaxAmount = opts.dustMaxAmount ?? 0.1;
  const dustMinCount = opts.dustMinCount ?? 10;

  const anomalies: TransactionAnomaly[] = [];
  if (txs.length < 5) return anomalies;

  for (const [i, tx] of txs.entries()) {

    // 1. Amount spike vs the record's own baseline (exclude the record itself).
    const others = txs.filter((t, j) => j !== i);
    const med = median(others.map((t) => Math.abs(t.amount)));
    const m = mad(others.map((t) => Math.abs(t.amount)), med) || 1;
    const z = Math.abs(Math.abs(tx.amount) - med) / (1.4826 * m);
    if (z >= spikeThreshold) {
      anomalies.push({ txHash: tx.txHash, reason: 'amount_spike', score: z, record: tx });
    }

    // 2. Velocity burst within the sliding window.
    if (i >= velocityWindow) {
      const window = txs.slice(Math.max(0, i - velocityWindow), i);
      const sameSender = window.filter((t) => t.sender === tx.sender).length;
      if (sameSender >= velocityThreshold - 1) {
        const burstFrom = txs.slice(Math.max(0, i - velocityWindow * 2), i + 1).filter(
          (t) => t.sender === tx.sender
        ).length;
        if (burstFrom >= velocityThreshold) {
          anomalies.push({
            txHash: tx.txHash,
            reason: 'velocity_burst',
            score: burstFrom,
            record: tx,
          });
        }
      }
    }
  }

  // 3. Dusting: count small credits per recipient.
  const dustCount = new Map<string, { count: number; total: number }>();
  for (const tx of txs) {
    if (tx.amount > 0 && tx.amount <= dustMaxAmount) {
      const acc = dustCount.get(tx.recipient) ?? { count: 0, total: 0 };
      dustCount.set(tx.recipient, { count: acc.count + 1, total: acc.total + tx.amount });
    }
  }
  for (const [recipient, acc] of dustCount) {
    if (acc.count >= dustMinCount) {
      anomalies.push({
        txHash: txs.find((t) => t.recipient === recipient && t.amount <= dustMaxAmount)?.txHash ?? '',
        reason: 'dusting',
        score: acc.count,
        record: { sender: 'many', recipient, amount: acc.total },
      });
    }
  }

  return anomalies.sort((a, b) => b.score - a.score);
}

function median(values: number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[mid - 1]! + sorted[mid]!) / 2 : sorted[mid]!;
}

function mad(values: number[], center: number): number {
  if (values.length === 0) return 0;
  const deviations = values.map((v) => Math.abs(v - center));
  return median(deviations);
}