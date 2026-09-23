import { Router } from 'express';
import { body, param, query } from 'express-validator';
import * as reinvestmentController from '../controllers/reinvestment.controller';
import { validateRequest } from '../middleware/validation';
import { authenticateToken } from '../middleware/auth';

const router: Router = Router();

const createPlanValidation = [
  body('userAddress').optional().isString().withMessage('userAddress must be a string'),
  body('sourcePool').isString().notEmpty().withMessage('sourcePool is required'),
  body('strategy')
    .isIn(['same_pool', 'best_apy', 'weighted'])
    .withMessage('strategy must be one of: same_pool, best_apy, weighted'),
  body('schedule')
    .isIn(['real_time', 'daily', 'weekly', 'threshold'])
    .withMessage('schedule must be one of: real_time, daily, weekly, threshold'),
  body('thresholdAmount').isString().notEmpty().withMessage('thresholdAmount is required'),
  body('weightedTargets').optional().isArray().withMessage('weightedTargets must be an array'),
];

const planIdParamValidation = [param('planId').isString().notEmpty().withMessage('planId is required')];

const userAddressParamValidation = [
  param('userAddress').isString().notEmpty().withMessage('userAddress is required'),
];

const pauseResumeValidation = [
  ...planIdParamValidation,
  body('userAddress').optional().isString().withMessage('userAddress must be a string'),
];

const sweepValidation = [
  ...planIdParamValidation,
  body('earnedAmount').isString().notEmpty().withMessage('earnedAmount is required'),
  body('estimatedGasCost').isString().notEmpty().withMessage('estimatedGasCost is required'),
  body('poolPaused').isBoolean().withMessage('poolPaused must be a boolean'),
  body('targetPool').optional().isString(),
  body('txHash')
    .isString()
    .notEmpty()
    .withMessage('txHash is required')
    .matches(/^[0-9a-fA-F]{64}$/)
    .withMessage('txHash must be a valid 64-character hex transaction hash'),
];

const analyticsValidation = [
  ...planIdParamValidation,
  query('assumedApyBps').optional().isInt({ min: 0, max: 10_000 }),
];

router.post('/plan', authenticateToken, createPlanValidation, validateRequest, reinvestmentController.createPlan);
router.get('/plan/:planId', planIdParamValidation, validateRequest, reinvestmentController.getPlan);
router.get(
  '/plans/:userAddress',
  userAddressParamValidation,
  validateRequest,
  reinvestmentController.getUserPlans
);
router.post('/plan/:planId/pause', authenticateToken, pauseResumeValidation, validateRequest, reinvestmentController.pausePlan);
router.post('/plan/:planId/resume', authenticateToken, pauseResumeValidation, validateRequest, reinvestmentController.resumePlan);
router.post('/plan/:planId/sweep', authenticateToken, sweepValidation, validateRequest, reinvestmentController.recordSweep);
router.get('/plan/:planId/history', planIdParamValidation, validateRequest, reinvestmentController.getHistory);
router.get(
  '/plan/:planId/analytics',
  analyticsValidation,
  validateRequest,
  reinvestmentController.getAnalytics
);

export default router;
