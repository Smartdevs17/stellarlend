// ════════════════════════════════════════════════════════════════
// BATCH TRANSACTION SERVICE - Frontend transaction batching
// ════════════════════════════════════════════════════════════════

import { Horizon } from "@stellar/js-stellar-sdk";

/**
 * Represents a single transaction in a batch
 */
export interface BatchTransaction {
  functionName: string;
  params: any[];
  dependsOn?: number;
  required: boolean;
}

/**
 * Result of executing a batch operation
 */
export interface OperationResult {
  index: number;
  success: boolean;
  result?: any;
  error?: string;
}

/**
 * Complete batch result
 */
export interface BatchExecutionResult {
  batchId: string;
  totalOperations: number;
  successfulOperations: number;
  failedOperations: number;
  results: OperationResult[];
  atomic: boolean;
  gasEstimate: number;
}

/**
 * Batch Transaction Service - Handles transaction batching
 */
export class BatchTransactionService {
  private server: Horizon.Server;
  private pendingTransactions: BatchTransaction[] = [];
  private maxBatchSize: number = 10;
  private autoExecute: boolean = false;
  private gasPerOperation: number = 100_000;

  constructor(serverUrl: string, maxBatchSize: number = 10) {
    this.server = new Horizon.Server(serverUrl);
    this.maxBatchSize = maxBatchSize;
  }

  /**
   * Add transaction to batch
   */
  addTransaction(
    functionName: string,
    params: any[],
    required: boolean = true
  ): boolean {
    // Check if batch is full
    if (this.pendingTransactions.length >= this.maxBatchSize) {
      console.warn("Batch is full, cannot add more transactions");
      return false;
    }

    const transaction: BatchTransaction = {
      functionName,
      params,
      required,
    };

    this.pendingTransactions.push(transaction);
    console.log(
      `Added transaction. Pending: ${this.pendingTransactions.length}/${this.maxBatchSize}`
    );

    return true;
  }

  /**
   * Add transaction with dependency on another operation
   */
  addTransactionWithDependency(
    functionName: string,
    params: any[],
    dependsOn: number,
    required: boolean = true
  ): boolean {
    if (this.pendingTransactions.length >= this.maxBatchSize) {
      return false;
    }

    // Validate dependency
    if (dependsOn >= this.pendingTransactions.length) {
      console.error("Invalid dependency: index out of range");
      return false;
    }

    const transaction: BatchTransaction = {
      functionName,
      params,
      dependsOn,
      required,
    };

    this.pendingTransactions.push(transaction);
    return true;
  }

  /**
   * Get pending transactions count
   */
  getPendingCount(): number {
    return this.pendingTransactions.length;
  }

  /**
   * Is batch ready to execute?
   */
  isBatchReady(): boolean {
    return this.pendingTransactions.length >= this.maxBatchSize;
  }

  /**
   * Get current pending batch
   */
  getPendingBatch(): BatchTransaction[] {
    return [...this.pendingTransactions];
  }

  /**
   * Simulate batch execution
   */
  async simulateBatch(
    accountId: string
  ): Promise<BatchExecutionResult> {
    console.log("Simulating batch execution...");

    const totalGas = this.pendingTransactions.length * this.gasPerOperation;
    const batchId = this.generateBatchId();

    const results: OperationResult[] = this.pendingTransactions.map(
      (tx, index) => ({
        index,
        success: true,
        result: null,
      })
    );

    return {
      batchId,
      totalOperations: this.pendingTransactions.length,
      successfulOperations: this.pendingTransactions.length,
      failedOperations: 0,
      results,
      atomic: false,
      gasEstimate: totalGas,
    };
  }

  /**
   * Execute batch synchronously
   */
  async executeBatch(
    accountId: string,
    atomic: boolean = true
  ): Promise<BatchExecutionResult> {
    console.log(
      `Executing batch with ${this.pendingTransactions.length} operations (atomic: ${atomic})...`
    );

    if (this.pendingTransactions.length === 0) {
      throw new Error("No transactions to execute");
    }

    const results: OperationResult[] = [];
    let successCount = 0;
    let failCount = 0;
    let totalGas = 0;
    let shouldStop = false;

    // Execute each transaction
    for (let i = 0; i < this.pendingTransactions.length; i++) {
      const tx = this.pendingTransactions[i];

      // Check if we should stop (atomic mode)
      if (shouldStop && atomic) {
        results.push({
          index: i,
          success: false,
          error: "Skipped due to atomic failure",
        });
        failCount++;
        continue;
      }

      // Check dependencies
      if (tx.dependsOn !== undefined) {
        const dependencyResult = results[tx.dependsOn];
        if (!dependencyResult.success) {
          results.push({
            index: i,
            success: false,
            error: "Dependency failed",
          });
          failCount++;

          if (tx.required) {
            shouldStop = true;
          }
          continue;
        }
      }

      // Execute transaction
      try {
        console.log(`Executing: ${tx.functionName}`);
        
        // Simulate execution
        const result = await this.executeTransaction(tx);
        const gasUsed = this.gasPerOperation;

        results.push({
          index: i,
          success: true,
          result,
        });

        successCount++;
        totalGas += gasUsed;
      } catch (error) {
        console.error(`Transaction failed: ${tx.functionName}`, error);

        results.push({
          index: i,
          success: false,
          error: String(error),
        });

        failCount++;

        if (tx.required) {
          shouldStop = true;
        }
      }
    }

    const batchResult: BatchExecutionResult = {
      batchId: this.generateBatchId(),
      totalOperations: this.pendingTransactions.length,
      successfulOperations: successCount,
      failedOperations: failCount,
      results,
      atomic,
      gasEstimate: totalGas,
    };

    // Clear batch after execution
    this.pendingTransactions = [];

    console.log(
      `Batch complete: ${successCount}/${this.pendingTransactions.length} successful`
    );
    return batchResult;
  }

  /**
   * Execute single transaction (internal)
   */
  private async executeTransaction(tx: BatchTransaction): Promise<any> {
    // In real implementation, call contract function
    // For now, simulate with delay
    return new Promise((resolve) => {
      setTimeout(() => {
        resolve({ success: true });
      }, 100);
    });
  }

  /**
   * Clear pending batch
   */
  clearBatch(): void {
    this.pendingTransactions = [];
    console.log("Batch cleared");
  }

  /**
   * Get gas estimate for pending batch
   */
  getGasEstimate(): number {
    return this.pendingTransactions.length * this.gasPerOperation;
  }

  /**
   * Get batch summary
   */
  getBatchSummary(): {
    pending: number;
    maxSize: number;
    estimatedGas: number;
    isFull: boolean;
  } {
    return {
      pending: this.pendingTransactions.length,
      maxSize: this.maxBatchSize,
      estimatedGas: this.getGasEstimate(),
      isFull: this.isBatchReady(),
    };
  }

  /**
   * Set maximum batch size
   */
  setMaxBatchSize(size: number): void {
    this.maxBatchSize = size;
    console.log(`Max batch size set to: ${size}`);
  }

  /**
   * Generate unique batch ID
   */
  private generateBatchId(): string {
    return `batch_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }
}

// Export for use in React components
export default BatchTransactionService;