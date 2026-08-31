import { Request, Response, NextFunction } from 'express';
import { recoveryService } from '../services/recovery.service';
import logger from '../utils/logger';

export const getGuardians = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const config = await recoveryService.getGuardians();
    return res.status(200).json({ success: true, ...config });
  } catch (error) {
    next(error);
    return;
  }
};

export const setGuardians = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { callerAddress, guardians, threshold } = req.body as any;

    if (!callerAddress || !guardians || !Array.isArray(guardians) || threshold === undefined) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: callerAddress, guardians (array), threshold',
      });
    }

    const result = await recoveryService.setGuardians({ callerAddress, guardians, threshold });
    logger.info('Guardians set', { caller: callerAddress, count: guardians.length });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const addGuardian = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { callerAddress, guardianAddress } = req.body as any;

    if (!callerAddress || !guardianAddress) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: callerAddress, guardianAddress',
      });
    }

    const result = await recoveryService.addGuardian({ callerAddress, guardianAddress });
    logger.info('Guardian added', { caller: callerAddress, guardian: guardianAddress });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const removeGuardian = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { callerAddress, guardianAddress } = req.body as any;

    if (!callerAddress || !guardianAddress) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: callerAddress, guardianAddress',
      });
    }

    const result = await recoveryService.removeGuardian({ callerAddress, guardianAddress });
    logger.info('Guardian removed', { caller: callerAddress, guardian: guardianAddress });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const setThreshold = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { callerAddress, threshold } = req.body as any;

    if (!callerAddress || threshold === undefined) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: callerAddress, threshold',
      });
    }

    const result = await recoveryService.setThreshold({ callerAddress, threshold });
    logger.info('Guardian threshold set', { caller: callerAddress, threshold });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const startRecovery = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { initiatorAddress, oldAdminAddress, newAdminAddress } = req.body as any;

    if (!initiatorAddress || !oldAdminAddress || !newAdminAddress) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: initiatorAddress, oldAdminAddress, newAdminAddress',
      });
    }

    const result = await recoveryService.startRecovery({
      initiatorAddress,
      oldAdminAddress,
      newAdminAddress,
    });

    logger.info('Recovery started', { initiator: initiatorAddress });
    return res.status(201).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const approveRecovery = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { approverAddress } = req.body as any;

    if (!approverAddress) {
      return res.status(400).json({
        success: false,
        error: 'Missing required field: approverAddress',
      });
    }

    const result = await recoveryService.approveRecovery({ approverAddress });
    logger.info('Recovery approved', { approver: approverAddress });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const executeRecovery = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { executorAddress } = req.body as any;

    if (!executorAddress) {
      return res.status(400).json({
        success: false,
        error: 'Missing required field: executorAddress',
      });
    }

    const result = await recoveryService.executeRecovery({ executorAddress });
    logger.info('Recovery executed', { executor: executorAddress });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const cancelRecovery = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { callerAddress } = req.body as any;

    if (!callerAddress) {
      return res.status(400).json({
        success: false,
        error: 'Missing required field: callerAddress',
      });
    }

    const result = await recoveryService.cancelRecovery({ callerAddress });
    logger.info('Recovery cancelled', { caller: callerAddress });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const getRecoveryRequest = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const request = await recoveryService.getRecoveryRequest();

    if (!request) {
      return res.status(200).json({ success: true, request: null });
    }

    return res.status(200).json({ success: true, request });
  } catch (error) {
    next(error);
    return;
  }
};

export const getRecoveryApprovals = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const approvals = await recoveryService.getRecoveryApprovals();
    return res.status(200).json({ success: true, approvals });
  } catch (error) {
    next(error);
    return;
  }
};
