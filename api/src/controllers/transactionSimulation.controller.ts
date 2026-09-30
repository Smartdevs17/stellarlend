import { Request, Response, NextFunction } from 'express';
import {
  parseSimulationRequest,
  transactionSimulationService,
} from '../services/transactionSimulation.service';
import { estimateFees, parseFeeMarginPercent } from '../services/sorobanFees';
import { ApiError, ErrorCode } from '../utils/errors';

/**
 * POST /api/lending/simulate
 * Simulate a transaction (base64 envelope or lending operation spec) and
 * return the normalized Soroban RPC outcome.
 */
export const simulateTransaction = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const request = parseSimulationRequest(req.body);
    const result = await transactionSimulationService.simulate(request);
    res.status(200).json(result);
  } catch (error) {
    next(error);
  }
};

/**
 * POST /api/gas/estimate-transaction
 * Estimate the fees a user's transaction will pay, derived from one simulation.
 */
export const estimateTransactionFees = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const request = parseSimulationRequest(req.body);
    const feeMarginPercent = parseFeeMarginPercent(
      (req.body as Record<string, unknown> | undefined)?.feeMarginPercent
    );
    const simulation = await transactionSimulationService.simulate(request);

    if (simulation.status === 'error') {
      throw new ApiError(
        422,
        'Transaction simulation failed, fees cannot be estimated',
        ErrorCode.CONTRACT_ERROR,
        {
          simulationError: simulation.error,
          events: simulation.events,
          latestLedger: simulation.latestLedger,
        }
      );
    }

    const fees = estimateFees({
      minResourceFee: simulation.minResourceFee ?? '0',
      operationCount: simulation.operationCount,
      feeMarginPercent,
    });

    res.status(200).json({
      success: true,
      status: simulation.status,
      sourceAccount: simulation.sourceAccount,
      operationCount: simulation.operationCount,
      latestLedger: simulation.latestLedger,
      fees,
      resources: simulation.resources,
      memoryBytes: simulation.memoryBytes,
      restoreRequired: simulation.status === 'restore_required',
      restorePreamble: simulation.restorePreamble,
      cached: simulation.cached,
      simulatedAt: simulation.simulatedAt,
    });
  } catch (error) {
    next(error);
  }
};
