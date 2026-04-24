import { Router } from 'express';
import { AccountingController } from '../controllers/accounting.controller';
import { validateRequest } from '../middleware/validation';
import { body, param, query } from 'express-validator';

const router = Router();
const accountingController = new AccountingController();

/**
 * @route   POST /api/accounting/subscriptions
 * @desc    Create a new subscription
 * @access  Private
 */
router.post(
  '/subscriptions',
  [
    body('merchantId').isString().notEmpty().withMessage('Merchant ID is required'),
    body('totalAmount').isString().notEmpty().withMessage('Total amount is required'),
    body('startTime').isInt({ min: 0 }).withMessage('Start time must be a positive integer'),
    validateRequest,
  ],
  accountingController.createSubscription
);

/**
 * @route   POST /api/accounting/subscriptions/:subscriptionId/configure
 * @desc    Configure revenue recognition rule
 * @access  Private
 */
router.post(
  '/subscriptions/:subscriptionId/configure',
  [
    param('subscriptionId').isInt({ min: 1 }).withMessage('Invalid subscription ID'),
    body('method').isInt({ min: 0, max: 2 }).withMessage('Invalid recognition method'),
    body('recognitionPeriod')
      .isInt({ min: 1 })
      .withMessage('Recognition period must be positive'),
    validateRequest,
  ],
  accountingController.configureRecognitionRule
);

/**
 * @route   POST /api/accounting/subscriptions/:subscriptionId/recognize
 * @desc    Recognize revenue for a subscription
 * @access  Private
 */
router.post(
  '/subscriptions/:subscriptionId/recognize',
  [param('subscriptionId').isInt({ min: 1 }).withMessage('Invalid subscription ID'), validateRequest],
  accountingController.recognizeRevenue
);

/**
 * @route   GET /api/accounting/merchants/:merchantId/deferred-revenue
 * @desc    Get deferred revenue for a merchant
 * @access  Private
 */
router.get(
  '/merchants/:merchantId/deferred-revenue',
  [param('merchantId').isString().notEmpty().withMessage('Merchant ID is required'), validateRequest],
  accountingController.getDeferredRevenue
);

/**
 * @route   GET /api/accounting/subscriptions/:subscriptionId/schedule
 * @desc    Get revenue schedule for a subscription
 * @access  Private
 */
router.get(
  '/subscriptions/:subscriptionId/schedule',
  [param('subscriptionId').isInt({ min: 1 }).withMessage('Invalid subscription ID'), validateRequest],
  accountingController.getRevenueSchedule
);

/**
 * @route   GET /api/accounting/merchants/:merchantId/analytics
 * @desc    Get revenue analytics for a merchant
 * @access  Private
 */
router.get(
  '/merchants/:merchantId/analytics',
  [
    param('merchantId').isString().notEmpty().withMessage('Merchant ID is required'),
    query('startTime').isInt({ min: 0 }).withMessage('Start time must be a positive integer'),
    query('endTime').isInt({ min: 0 }).withMessage('End time must be a positive integer'),
    validateRequest,
  ],
  accountingController.getRevenueAnalytics
);

/**
 * @route   PUT /api/accounting/subscriptions/:subscriptionId/modify
 * @desc    Modify contract amount
 * @access  Private
 */
router.put(
  '/subscriptions/:subscriptionId/modify',
  [
    param('subscriptionId').isInt({ min: 1 }).withMessage('Invalid subscription ID'),
    body('newAmount').isString().notEmpty().withMessage('New amount is required'),
    validateRequest,
  ],
  accountingController.modifyContract
);

/**
 * @route   POST /api/accounting/subscriptions/:subscriptionId/cancel
 * @desc    Cancel a subscription
 * @access  Private
 */
router.post(
  '/subscriptions/:subscriptionId/cancel',
  [param('subscriptionId').isInt({ min: 1 }).withMessage('Invalid subscription ID'), validateRequest],
  accountingController.cancelSubscription
);

/**
 * @route   GET /api/accounting/subscriptions/:subscriptionId
 * @desc    Get subscription state
 * @access  Private
 */
router.get(
  '/subscriptions/:subscriptionId',
  [param('subscriptionId').isInt({ min: 1 }).withMessage('Invalid subscription ID'), validateRequest],
  accountingController.getSubscriptionState
);

/**
 * @route   GET /api/accounting/merchants/:merchantId/subscriptions
 * @desc    Get merchant subscriptions
 * @access  Private
 */
router.get(
  '/merchants/:merchantId/subscriptions',
  [param('merchantId').isString().notEmpty().withMessage('Merchant ID is required'), validateRequest],
  accountingController.getMerchantSubscriptions
);

export default router;
