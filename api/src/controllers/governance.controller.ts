import { Request, Response, NextFunction } from 'express';
import { governanceService } from '../services/governance.service';
import logger from '../utils/logger';

export const getTimelockConfig = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const config = await governanceService.getTimelockConfig();
    return res.status(200).json({ success: true, config });
  } catch (error) {
    next(error);
    return;
  }
};

export const queueOperation = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { proposerAddress, proposalType, description, customDelay } = req.body as any;

    if (!proposerAddress || !proposalType || !description) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: proposerAddress, proposalType, description',
      });
    }

    const result = await governanceService.queueOperation({
      proposerAddress,
      proposalType,
      description,
      customDelay,
    });

    logger.info('Timelock operation queued', { operationId: result.operationId });
    return res.status(201).json({ success: true, ...result });
  } catch (error) {
    next(error);
    return;
  }
};

export const queueBatchOperation = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { proposerAddress, actions, description, customDelay } = req.body as any;

    if (!proposerAddress || !actions || !Array.isArray(actions) || !description) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: proposerAddress, actions (array), description',
      });
    }

    const result = await governanceService.queueBatchOperation({
      proposerAddress,
      actions,
      description,
      customDelay,
    });

    logger.info('Batch timelock operation queued', { operationId: result.operationId });
    return res.status(201).json({ success: true, ...result });
  } catch (error) {
    next(error);
    return;
  }
};

export const executeOperation = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { executorAddress, operationId } = req.body as any;

    if (!executorAddress || operationId === undefined) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: executorAddress, operationId',
      });
    }

    const result = await governanceService.executeOperation({ executorAddress, operationId });
    logger.info('Timelock operation executed', { operationId });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const executeBatchOperation = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { executorAddress, operationId } = req.body as any;

    if (!executorAddress || operationId === undefined) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: executorAddress, operationId',
      });
    }

    const result = await governanceService.executeBatchOperation({ executorAddress, operationId });
    logger.info('Batch timelock operation executed', { operationId });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const cancelOperation = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { callerAddress, operationId } = req.body as any;

    if (!callerAddress || operationId === undefined) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: callerAddress, operationId',
      });
    }

    const result = await governanceService.cancelOperation({ callerAddress, operationId });
    logger.info('Timelock operation cancelled', { operationId });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const getOperation = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const operationId = parseInt(req.params.operationId, 10);

    if (isNaN(operationId)) {
      return res.status(400).json({ success: false, error: 'Invalid operationId' });
    }

    const operation = await governanceService.getOperation(operationId);

    if (!operation) {
      return res.status(404).json({ success: false, error: 'Operation not found' });
    }

    return res.status(200).json({ success: true, operation });
  } catch (error) {
    next(error);
    return;
  }
};

export const getPendingOperations = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const operations = await governanceService.getPendingOperations();
    return res.status(200).json({ success: true, operations });
  } catch (error) {
    next(error);
    return;
  }
};

export const getQueue = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const queue = await governanceService.getQueue();
    return res.status(200).json({ success: true, queue });
  } catch (error) {
    next(error);
    return;
  }
};

export const updateTimelockConfig = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { adminAddress, minDelay, maxDelay, defaultDelay, gracePeriod } = req.body as any;

    if (!adminAddress || minDelay === undefined || maxDelay === undefined || defaultDelay === undefined || gracePeriod === undefined) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: adminAddress, minDelay, maxDelay, defaultDelay, gracePeriod',
      });
    }

    const result = await governanceService.updateConfig({
      adminAddress,
      minDelay,
      maxDelay,
      defaultDelay,
      gracePeriod,
    });

    logger.info('Timelock config updated', { admin: adminAddress });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const setActionTypeDelay = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { adminAddress, actionTypeId, delay } = req.body as any;

    if (!adminAddress || actionTypeId === undefined || delay === undefined) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: adminAddress, actionTypeId, delay',
      });
    }

    const result = await governanceService.setActionTypeDelay(adminAddress, actionTypeId, delay);
    logger.info('Action type delay set', { admin: adminAddress, actionTypeId });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const getActionTypeDelay = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const actionTypeId = parseInt(req.params.actionTypeId, 10);

    if (isNaN(actionTypeId)) {
      return res.status(400).json({ success: false, error: 'Invalid actionTypeId' });
    }

    const delay = await governanceService.getActionTypeDelay(actionTypeId);
    return res.status(200).json({ success: true, delay });
  } catch (error) {
    next(error);
    return;
  }
};

export const guardianApproveEmergency = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { guardianAddress, operationId } = req.body as any;

    if (!guardianAddress || operationId === undefined) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: guardianAddress, operationId',
      });
    }

    const result = await governanceService.guardianApproveEmergency({ guardianAddress, operationId });
    logger.info('Guardian emergency approval submitted', { operationId });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const guardianEmergencyExecute = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const { executorAddress, operationId } = req.body as any;

    if (!executorAddress || operationId === undefined) {
      return res.status(400).json({
        success: false,
        error: 'Missing required fields: executorAddress, operationId',
      });
    }

    const result = await governanceService.guardianEmergencyExecute({ executorAddress, operationId });
    logger.info('Guardian emergency execution submitted', { operationId });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};

export const cleanQueue = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const result = await governanceService.cleanQueue();
    logger.info('Timelock queue cleaned', { removed: result.removed });
    return res.status(200).json(result);
  } catch (error) {
    next(error);
    return;
  }
};
