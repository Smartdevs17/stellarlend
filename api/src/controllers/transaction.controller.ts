import { Response, NextFunction } from 'express';
import { transactionBuilderService } from '../services/transactionBuilder.service';
import type { AuthRequest } from '../middleware/auth';
import type {
  CreateTransactionRequest,
  ApproveStepRequest,
  RejectStepRequest,
} from '../types/transaction';
import { ForbiddenError, UnauthorizedError } from '../utils/errors';

function getAuthenticatedCaller(req: AuthRequest): string {
  const address = req.user?.address;
  if (!address) {
    throw new UnauthorizedError('User authentication required');
  }
  return address;
}

/**
 * Creates a new multi-step transaction for the authenticated caller.
 */
export const createTransaction = async (req: AuthRequest, res: Response, next: NextFunction) => {
  try {
    const callerAddress = getAuthenticatedCaller(req);
    const body = req.body as CreateTransactionRequest;
    if (body.userAddress && body.userAddress !== callerAddress) {
      throw new ForbiddenError('Cannot create transaction for another user');
    }
    const tx = transactionBuilderService.create(body, callerAddress);
    return res.status(201).json({ success: true, transaction: tx });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * Prepares an unsigned transaction XDR for a specific step.
 */
export const prepareStep = async (req: AuthRequest, res: Response, next: NextFunction) => {
  try {
    const callerAddress = getAuthenticatedCaller(req);
    const { txId, stepId } = req.params;
    const tx = await transactionBuilderService.prepareStep(txId!, stepId!, callerAddress);
    return res.status(200).json({ success: true, transaction: tx });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * Submits a signed transaction for step approval and on-chain execution.
 */
export const approveStep = async (req: AuthRequest, res: Response, next: NextFunction) => {
  try {
    const callerAddress = getAuthenticatedCaller(req);
    const body = req.body as ApproveStepRequest;
    const tx = await transactionBuilderService.approveStep(body, callerAddress);
    return res.status(200).json({ success: true, transaction: tx });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * Rejects a transaction step and terminates the workflow.
 */
export const rejectStep = async (req: AuthRequest, res: Response, next: NextFunction) => {
  try {
    const callerAddress = getAuthenticatedCaller(req);
    const body = req.body as RejectStepRequest;
    const tx = transactionBuilderService.rejectStep(body, callerAddress);
    return res.status(200).json({ success: true, transaction: tx });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * Retrieves a multi-step transaction owned by the authenticated caller.
 */
export const getTransaction = async (req: AuthRequest, res: Response, next: NextFunction) => {
  try {
    const callerAddress = getAuthenticatedCaller(req);
    const { txId } = req.params;
    const tx = transactionBuilderService.getTransaction(txId!, callerAddress);
    return res.status(200).json({ success: true, transaction: tx });
  } catch (err) {
    next(err);
    return;
  }
};

/**
 * Lists all multi-step transactions owned by the authenticated caller.
 */
export const listUserTransactions = async (req: AuthRequest, res: Response, next: NextFunction) => {
  try {
    const callerAddress = getAuthenticatedCaller(req);
    const { userAddress } = req.params;
    if (userAddress !== callerAddress) {
      throw new ForbiddenError('Cannot list transactions for another user');
    }
    const transactions = transactionBuilderService.listForUser(userAddress!, callerAddress);
    return res.status(200).json({ success: true, transactions, total: transactions.length });
  } catch (err) {
    next(err);
    return;
  }
};
