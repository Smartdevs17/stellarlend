import { randomUUID } from 'crypto';
import type {
  MultiStepTransaction,
  TransactionStep,
  CreateTransactionRequest,
  ApproveStepRequest,
  RejectStepRequest,
} from '../types/transaction';
import { StellarService } from './stellar.service';
import logger from '../utils/logger';
import {
  ValidationError,
  NotFoundError,
  ForbiddenError,
  ExpiredError,
} from '../utils/errors';

const DEFAULT_TTL_SECONDS = 600;
const MAX_STEPS = 10;

const transactions = new Map<string, MultiStepTransaction>();

function now(): string {
  return new Date().toISOString();
}

function isExpired(tx: MultiStepTransaction): boolean {
  return Date.now() > new Date(tx.expiresAt).getTime();
}

/**
 * Resets the in-memory transaction store for testing isolation.
 */
export function resetTransactionStore(): void {
  transactions.clear();
}

/**
 * Manages multi-step transaction preparation, execution, and state transitions.
 */
export class TransactionBuilderService {
  private stellar = new StellarService();

  /**
   * Initializes a multi-step transaction workflow.
   */
  create(req: CreateTransactionRequest, callerAddress?: string): MultiStepTransaction {
    if (callerAddress && req.userAddress && req.userAddress !== callerAddress) {
      throw new ForbiddenError('Cannot create transaction for another user');
    }

    const userAddress = callerAddress ?? req.userAddress;
    if (!userAddress) {
      throw new ValidationError('User address is required');
    }

    if (!req.steps || req.steps.length === 0) {
      throw new ValidationError('At least one step is required');
    }
    if (req.steps.length > MAX_STEPS) {
      throw new ValidationError(`Maximum ${MAX_STEPS} steps allowed`);
    }

    const txId = randomUUID();
    const ttl = req.ttlSeconds ?? DEFAULT_TTL_SECONDS;
    const timestamp = now();

    const steps: TransactionStep[] = req.steps.map((s, i) => ({
      stepId: randomUUID(),
      index: i,
      operation: s.operation,
      userAddress,
      amount: s.amount,
      assetAddress: s.assetAddress,
      status: 'pending',
      createdAt: timestamp,
      updatedAt: timestamp,
    }));

    const tx: MultiStepTransaction = {
      txId,
      userAddress,
      description: req.description ?? `Multi-step transaction (${steps.length} steps)`,
      steps,
      currentStepIndex: 0,
      status: 'building',
      metadata: {},
      createdAt: timestamp,
      updatedAt: timestamp,
      expiresAt: new Date(Date.now() + ttl * 1000).toISOString(),
    };

    transactions.set(txId, tx);
    logger.info('Multi-step transaction created', { txId, steps: steps.length });
    return tx;
  }

  /**
   * Prepares the next step by generating an unsigned transaction XDR.
   */
  async prepareStep(txId: string, stepId: string, callerAddress?: string): Promise<MultiStepTransaction> {
    const tx = this.assertActive(txId, callerAddress);
    const step = this.findStep(tx, stepId);

    if (step.index !== tx.currentStepIndex) {
      throw new ValidationError('Steps must be prepared in order');
    }
    if (step.status !== 'pending') {
      throw new ValidationError(`Step is already in status: ${step.status}`);
    }

    const unsignedXdr = await this.stellar.buildUnsignedTransaction(
      step.operation,
      step.userAddress,
      step.assetAddress,
      step.amount
    );

    step.unsignedXdr = unsignedXdr;
    step.status = 'approved';
    step.updatedAt = now();
    tx.status = 'pending_approval';
    tx.updatedAt = now();

    transactions.set(txId, tx);
    logger.info('Step prepared', { txId, stepId });
    return tx;
  }

  /**
   * Submits a signed step transaction on-chain.
   */
  async approveStep(req: ApproveStepRequest, callerAddress?: string): Promise<MultiStepTransaction> {
    const tx = this.assertActive(req.txId, callerAddress);
    const step = this.findStep(tx, req.stepId);

    if (step.status !== 'approved') {
      throw new ValidationError('Step must be in approved status before execution');
    }

    step.signedXdr = req.signedXdr;
    step.status = 'executing';
    step.updatedAt = now();
    tx.status = 'executing';
    tx.updatedAt = now();
    transactions.set(req.txId, tx);

    try {
      const result = await this.stellar.submitTransaction(req.signedXdr);
      step.txHash = result.transactionHash;
      step.status = result.success ? 'completed' : 'failed';
      step.error = result.success ? undefined : result.error;
      step.updatedAt = now();

      if (result.success) {
        const nextIndex = step.index + 1;
        tx.currentStepIndex = nextIndex;
        tx.status = nextIndex >= tx.steps.length ? 'completed' : 'pending_approval';
      } else {
        tx.status = 'failed';
      }
    } catch (err: unknown) {
      step.status = 'failed';
      step.error = err instanceof Error ? err.message : String(err);
      step.updatedAt = now();
      tx.status = 'failed';
    }

    tx.updatedAt = now();
    transactions.set(req.txId, tx);
    logger.info('Step executed', { txId: req.txId, stepId: req.stepId, status: step.status });
    return tx;
  }

  /**
   * Rejects an in-progress step and marks the workflow as failed.
   */
  rejectStep(req: RejectStepRequest, callerAddress?: string): MultiStepTransaction {
    const tx = this.assertActive(req.txId, callerAddress);
    const step = this.findStep(tx, req.stepId);

    step.status = 'rejected';
    step.error = req.reason;
    step.updatedAt = now();
    tx.status = 'failed';
    tx.updatedAt = now();

    transactions.set(req.txId, tx);
    logger.info('Step rejected', { txId: req.txId, stepId: req.stepId });
    return tx;
  }

  /**
   * Retrieves a single transaction workflow by identifier.
   */
  getTransaction(txId: string, callerAddress?: string): MultiStepTransaction {
    const tx = transactions.get(txId);
    if (!tx) {
      throw new NotFoundError('Transaction not found');
    }
    if (callerAddress && tx.userAddress !== callerAddress) {
      throw new ForbiddenError('Cannot access transaction belonging to another user');
    }
    if (isExpired(tx) && tx.status !== 'completed' && tx.status !== 'failed') {
      this.markExpired(tx);
    }
    return tx;
  }

  /**
   * Lists all transactions associated with a user address.
   */
  listForUser(userAddress: string, callerAddress?: string): MultiStepTransaction[] {
    if (callerAddress && userAddress !== callerAddress) {
      throw new ForbiddenError('Cannot list transactions for another user');
    }
    const result: MultiStepTransaction[] = [];
    for (const tx of transactions.values()) {
      if (tx.userAddress !== userAddress) continue;
      if (isExpired(tx) && tx.status !== 'completed' && tx.status !== 'failed') {
        this.markExpired(tx);
      }
      result.push(tx);
    }
    return result.sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime());
  }

  /**
   * Cleans up expired and old terminal transactions.
   */
  cleanupExpired(): number {
    let count = 0;
    const cutoff = Date.now() - 24 * 60 * 60 * 1000;
    for (const [txId, tx] of transactions.entries()) {
      const isOldTerminal =
        (tx.status === 'completed' || tx.status === 'failed') &&
        new Date(tx.updatedAt).getTime() < cutoff;
      if (isOldTerminal || (isExpired(tx) && tx.status === 'expired')) {
        transactions.delete(txId);
        count++;
      }
    }
    if (count > 0) logger.info('Cleaned up expired transactions', { count });
    return count;
  }

  private assertActive(txId: string, callerAddress?: string): MultiStepTransaction {
    const tx = transactions.get(txId);
    if (!tx) throw new NotFoundError('Transaction not found');
    if (callerAddress && tx.userAddress !== callerAddress) {
      throw new ForbiddenError('Cannot access transaction belonging to another user');
    }
    if (isExpired(tx)) {
      this.markExpired(tx);
      throw new ExpiredError('Transaction has expired');
    }
    if (tx.status === 'completed') {
      throw new ValidationError('Transaction is already completed');
    }
    if (tx.status === 'failed') {
      throw new ValidationError('Transaction has failed');
    }
    return tx;
  }

  private findStep(tx: MultiStepTransaction, stepId: string): TransactionStep {
    const step = tx.steps.find((s) => s.stepId === stepId);
    if (!step) throw new NotFoundError('Step not found');
    return step;
  }

  private markExpired(tx: MultiStepTransaction): void {
    tx.status = 'expired';
    tx.steps.forEach((s) => {
      if (s.status === 'pending' || s.status === 'approved') {
        s.status = 'expired';
        s.updatedAt = now();
      }
    });
    tx.updatedAt = now();
    transactions.set(tx.txId, tx);
  }
}

export const transactionBuilderService = new TransactionBuilderService();

const cleanupInterval = setInterval(() => transactionBuilderService.cleanupExpired(), 15 * 60 * 1000);
cleanupInterval.unref();
