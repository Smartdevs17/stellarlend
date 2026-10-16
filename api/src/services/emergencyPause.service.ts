import { config } from '../config';
import logger from '../utils/logger';

export type PauseReason = 'manual' | 'auto-failure-threshold' | 'governance-vote' | 'oracle-failure';

export interface PauseState {
  paused: boolean;
  reason: PauseReason | null;
  since: number | null;
}

export interface QueuedWithdrawal {
  userAddress: string;
  assetAddress?: string;
  amount: string;
  queuedAt: string;
}

export interface EmergencyWithdrawalExecution {
  id: string;
  userAddress: string;
  assetAddress: string;
  requestedAmount: number;
  feeAmount: number;
  netAmount: number;
  feeSavings: number;
  standardFeeBps: number;
  emergencyFeeBps: number;
  txHash: string;
  timestamp: string;
  status: 'confirmed' | 'pending' | 'failed';
}

export interface EmergencyLimitsConfig {
  maxPerTransaction: number;
  maxDailyPerUser: number;
  maxDailyPoolDrain: number;
  cooldownPeriodSeconds: number;
}

export interface EmergencyAnalytics {
  totalEmergencyWithdrawn: number;
  totalEmergencyFeesCollected: number;
  totalFeeSavingsDelivered: number;
  totalWithdrawalCount: number;
  uniqueUsersAffected: number;
  hourlyDrainRate: number;
  assetBreakdown: Record<string, { totalAmount: number; count: number }>;
  systemHealth: 'normal' | 'emergency_active' | 'recovery';
}

export interface EmergencyReport {
  reportId: string;
  generatedAt: string;
  state: PauseState;
  analytics: EmergencyAnalytics;
  limits: EmergencyLimitsConfig;
  recentWithdrawals: EmergencyWithdrawalExecution[];
  auditTrail: EmergencyEvent[];
}

export interface EmergencyEvent {
  type: 'pause' | 'resume' | 'withdrawal' | 'emergency_withdrawal' | 'notification' | 'limits_update';
  timestamp: string;
  details: Record<string, unknown>;
}

export interface EmergencyNotification {
  id: string;
  message: string;
  severity: 'info' | 'warning' | 'critical';
  timestamp: string;
  read: boolean;
}

class EmergencyPauseService {
  private state: PauseState = { paused: false, reason: null, since: null };
  private consecutiveFailures = 0;
  private readonly withdrawalQueue: QueuedWithdrawal[] = [];
  private readonly emergencyHistory: EmergencyEvent[] = [];
  private readonly notifications: EmergencyNotification[] = [];
  private readonly emergencyWithdrawals: EmergencyWithdrawalExecution[] = [];
  private readonly userDailyWithdrawals = new Map<string, { total: number; resetAt: number }>();
  private notificationCounter = 0;

  // Emergency fee rates: standard 50 bps vs emergency 10 bps (80% discount)
  public readonly STANDARD_FEE_BPS = 50;
  public readonly EMERGENCY_FEE_BPS = 10;

  private limits: EmergencyLimitsConfig = {
    maxPerTransaction: 500_000,
    maxDailyPerUser: 1_000_000,
    maxDailyPoolDrain: 5_000_000,
    cooldownPeriodSeconds: 300,
  };

  isPaused(): PauseState {
    return { ...this.state };
  }

  recordSuccess(): void {
    this.consecutiveFailures = 0;
  }

  recordFailure(): void {
    this.consecutiveFailures += 1;
    if (this.consecutiveFailures >= config.emergency.autoPauseFailureThreshold) {
      this.pause('auto-failure-threshold');
    }
  }

  pause(reason: PauseReason): void {
    this.state = {
      paused: true,
      reason,
      since: Date.now(),
    };

    this.recordEvent('pause', { reason });
    this.addNotification(
      `Protocol paused: ${reason}. Emergency withdrawal mechanism active with reduced fees.`,
      reason === 'auto-failure-threshold' ? 'critical' : 'warning',
    );

    logger.info(`Emergency pause triggered: ${reason}`);
  }

  resume(): void {
    const previousReason = this.state.reason;
    this.state = { paused: false, reason: null, since: null };
    this.consecutiveFailures = 0;

    this.recordEvent('resume', { previousReason });
    this.addNotification('Protocol resumed normal operations.', 'info');

    logger.info('Emergency pause resumed');
  }

  /**
   * Execute an immediate emergency withdrawal with reduced fees and safety limit checks
   */
  executeEmergencyWithdrawal(params: {
    userAddress: string;
    assetAddress?: string;
    amount: number;
    txHash?: string;
  }): EmergencyWithdrawalExecution {
    const { userAddress, assetAddress = 'USDC', amount, txHash } = params;

    if (amount <= 0) {
      throw new Error('Withdrawal amount must be greater than 0');
    }

    // Check per-transaction limit
    if (this.limits.maxPerTransaction > 0 && amount > this.limits.maxPerTransaction) {
      throw new Error(
        `Amount ${amount} exceeds max emergency withdrawal limit of ${this.limits.maxPerTransaction} per transaction`,
      );
    }

    // Check daily user limit
    const now = Date.now();
    const userTracking = this.userDailyWithdrawals.get(userAddress) ?? { total: 0, resetAt: now + 86400000 };
    if (now > userTracking.resetAt) {
      userTracking.total = 0;
      userTracking.resetAt = now + 86400000;
    }
    if (this.limits.maxDailyPerUser > 0 && userTracking.total + amount > this.limits.maxDailyPerUser) {
      throw new Error(
        `User daily emergency limit reached (${this.limits.maxDailyPerUser}). Currently withdrawn: ${userTracking.total}`,
      );
    }

    // Calculate reduced emergency fee
    const standardFee = (amount * this.STANDARD_FEE_BPS) / 10_000;
    const feeAmount = (amount * this.EMERGENCY_FEE_BPS) / 10_000;
    const netAmount = amount - feeAmount;
    const feeSavings = standardFee - feeAmount;

    userTracking.total += amount;
    this.userDailyWithdrawals.set(userAddress, userTracking);

    const generatedTx = txHash || `tx_${Date.now()}_${Math.random().toString(36).substring(2, 9)}`;
    const execution: EmergencyWithdrawalExecution = {
      id: `em_wd_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`,
      userAddress,
      assetAddress,
      requestedAmount: amount,
      feeAmount,
      netAmount,
      feeSavings,
      standardFeeBps: this.STANDARD_FEE_BPS,
      emergencyFeeBps: this.EMERGENCY_FEE_BPS,
      txHash: generatedTx,
      timestamp: new Date().toISOString(),
      status: 'confirmed',
    };

    this.emergencyWithdrawals.push(execution);

    this.recordEvent('emergency_withdrawal', {
      userAddress,
      assetAddress,
      requestedAmount: amount,
      feeAmount,
      netAmount,
      feeSavings,
      txHash: generatedTx,
    });

    this.addNotification(
      `Emergency withdrawal executed for ${userAddress}: ${amount} ${assetAddress} (Fee: ${feeAmount} [reduced by ${feeSavings}])`,
      'warning',
    );

    logger.info(
      `Emergency withdrawal executed: ${userAddress} withdrew ${amount} ${assetAddress} (net: ${netAmount}, fee: ${feeAmount})`,
    );

    return execution;
  }

  /**
   * Fee preview helper
   */
  calculateEmergencyFee(amount: number) {
    const standardFee = (amount * this.STANDARD_FEE_BPS) / 10_000;
    const emergencyFee = (amount * this.EMERGENCY_FEE_BPS) / 10_000;
    return {
      requestedAmount: amount,
      standardFee,
      emergencyFee,
      savings: standardFee - emergencyFee,
      savingsPercent: '80%',
      netAmount: amount - emergencyFee,
    };
  }

  getLimits(): EmergencyLimitsConfig {
    return { ...this.limits };
  }

  updateLimits(newLimits: Partial<EmergencyLimitsConfig>): EmergencyLimitsConfig {
    this.limits = { ...this.limits, ...newLimits };
    this.recordEvent('limits_update', { limits: this.limits });
    logger.info('Emergency withdrawal limits updated', this.limits);
    return { ...this.limits };
  }

  getEmergencyAnalytics(): EmergencyAnalytics {
    let totalEmergencyWithdrawn = 0;
    let totalEmergencyFeesCollected = 0;
    let totalFeeSavingsDelivered = 0;
    const users = new Set<string>();
    const assetBreakdown: Record<string, { totalAmount: number; count: number }> = {};

    for (const w of this.emergencyWithdrawals) {
      totalEmergencyWithdrawn += w.requestedAmount;
      totalEmergencyFeesCollected += w.feeAmount;
      totalFeeSavingsDelivered += w.feeSavings;
      users.add(w.userAddress);

      if (!assetBreakdown[w.assetAddress]) {
        assetBreakdown[w.assetAddress] = { totalAmount: 0, count: 0 };
      }
      assetBreakdown[w.assetAddress].totalAmount += w.requestedAmount;
      assetBreakdown[w.assetAddress].count += 1;
    }

    const oneHourAgo = Date.now() - 3600000;
    const hourlyDrainRate = this.emergencyWithdrawals
      .filter((w) => new Date(w.timestamp).getTime() >= oneHourAgo)
      .reduce((sum, w) => sum + w.requestedAmount, 0);

    return {
      totalEmergencyWithdrawn,
      totalEmergencyFeesCollected,
      totalFeeSavingsDelivered,
      totalWithdrawalCount: this.emergencyWithdrawals.length,
      uniqueUsersAffected: users.size,
      hourlyDrainRate,
      assetBreakdown,
      systemHealth: this.state.paused ? 'emergency_active' : 'normal',
    };
  }

  generateEmergencyReport(): EmergencyReport {
    return {
      reportId: `report_${Date.now()}`,
      generatedAt: new Date().toISOString(),
      state: this.isPaused(),
      analytics: this.getEmergencyAnalytics(),
      limits: this.getLimits(),
      recentWithdrawals: this.emergencyWithdrawals.slice(-50),
      auditTrail: this.getEmergencyHistory(),
    };
  }

  queueWithdrawal(entry: Omit<QueuedWithdrawal, 'queuedAt'>): void {
    const queuedEntry: QueuedWithdrawal = {
      ...entry,
      queuedAt: new Date().toISOString(),
    };
    this.withdrawalQueue.push(queuedEntry);

    this.recordEvent('withdrawal', { userAddress: entry.userAddress, amount: entry.amount });
    this.addNotification(
      `Emergency withdrawal queued for ${entry.userAddress}: ${entry.amount}`,
      'warning',
    );

    logger.info(`Emergency withdrawal queued: ${entry.userAddress} - ${entry.amount}`);
  }

  drainWithdrawalQueue(): QueuedWithdrawal[] {
    const drained = [...this.withdrawalQueue];
    this.withdrawalQueue.length = 0;
    return drained;
  }

  getWithdrawalQueue(): QueuedWithdrawal[] {
    return [...this.withdrawalQueue];
  }

  getEmergencyWithdrawals(): EmergencyWithdrawalExecution[] {
    return [...this.emergencyWithdrawals];
  }

  getEmergencyHistory(): EmergencyEvent[] {
    return [...this.emergencyHistory];
  }

  getNotifications(): EmergencyNotification[] {
    return [...this.notifications];
  }

  getUnreadNotifications(): EmergencyNotification[] {
    return this.notifications.filter((n) => !n.read);
  }

  markNotificationRead(id: string): boolean {
    const notification = this.notifications.find((n) => n.id === id);
    if (notification) {
      notification.read = true;
      return true;
    }
    return false;
  }

  markAllNotificationsRead(): void {
    this.notifications.forEach((n) => {
      n.read = true;
    });
  }

  private recordEvent(type: EmergencyEvent['type'], details: Record<string, unknown>): void {
    this.emergencyHistory.push({
      type,
      timestamp: new Date().toISOString(),
      details,
    });
  }

  private addNotification(message: string, severity: EmergencyNotification['severity']): void {
    this.notificationCounter++;
    this.notifications.push({
      id: `notif-${this.notificationCounter}`,
      message,
      severity,
      timestamp: new Date().toISOString(),
      read: false,
    });
  }
}

export const emergencyPauseService = new EmergencyPauseService();
