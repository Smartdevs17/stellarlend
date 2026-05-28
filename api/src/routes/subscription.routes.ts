import { Router } from 'express';
import * as subscriptionController from '../controllers/subscription.controller';
import {
  importRequestValidation,
  merchantParamValidation,
} from '../middleware/validation';
import { requireRole } from '../middleware/rbac';

const router = Router();

// ─── Recurring Operations CRUD ─────────────────────────────────────────────────

/**
 * @openapi
 * /subscriptions:
 *   post:
 *     summary: Create a recurring subscription
 *     tags:
 *       - Subscriptions
 */
router.post('/', subscriptionController.createSubscription);

/**
 * @openapi
 * /subscriptions/{userAddress}:
 *   get:
 *     summary: List all subscriptions for a user
 *     tags:
 *       - Subscriptions
 *     parameters:
 *       - in: path
 *         name: userAddress
 *         required: true
 *         schema:
 *           type: string
 *       - in: query
 *         name: status
 *         schema:
 *           type: string
 *       - in: query
 *         name: action
 *         schema:
 *           type: string
 */
router.get('/:userAddress', subscriptionController.listSubscriptions);

/**
 * @openapi
 * /subscriptions/{userAddress}/{subscriptionId}:
 *   get:
 *     summary: Get a specific subscription
 *     tags:
 *       - Subscriptions
 */
router.get('/:userAddress/:subscriptionId', subscriptionController.getSubscription);

/**
 * @openapi
 * /subscriptions/{userAddress}/{subscriptionId}:
 *   patch:
 *     summary: Update a subscription
 *     tags:
 *       - Subscriptions
 */
router.patch('/:userAddress/:subscriptionId', subscriptionController.updateSubscription);

/**
 * @openapi
 * /subscriptions/{userAddress}/{subscriptionId}/pause:
 *   post:
 *     summary: Pause a subscription
 *     tags:
 *       - Subscriptions
 */
router.post('/:userAddress/:subscriptionId/pause', subscriptionController.pauseSubscription);

/**
 * @openapi
 * /subscriptions/{userAddress}/{subscriptionId}/resume:
 *   post:
 *     summary: Resume a paused subscription
 *     tags:
 *       - Subscriptions
 */
router.post('/:userAddress/:subscriptionId/resume', subscriptionController.resumeSubscription);

/**
 * @openapi
 * /subscriptions/{userAddress}/{subscriptionId}/cancel:
 *   post:
 *     summary: Cancel a subscription
 *     tags:
 *       - Subscriptions
 */
router.post('/:userAddress/:subscriptionId/cancel', subscriptionController.cancelSubscription);

// ─── Execution ─────────────────────────────────────────────────────────────────

/**
 * @openapi
 * /subscriptions/{userAddress}/{subscriptionId}/execute:
 *   post:
 *     summary: Trigger manual execution of a subscription
 *     tags:
 *       - Subscriptions
 */
router.post(
  '/:userAddress/:subscriptionId/execute',
  subscriptionController.triggerManualExecution
);

/**
 * @openapi
 * /subscriptions/execute-due:
 *   post:
 *     summary: Execute all due subscriptions (keeper endpoint)
 *     tags:
 *       - Subscriptions
 */
router.post(
  '/execute-due',
  requireRole('operator'),
  subscriptionController.executeDueSubscriptions
);

/**
 * @openapi
 * /subscriptions/{subscriptionId}/history:
 *   get:
 *     summary: Get execution history for a subscription
 *     tags:
 *       - Subscriptions
 */
router.get('/:subscriptionId/history', subscriptionController.getExecutionHistory);

/**
 * @openapi
 * /subscriptions/{userAddress}/analytics:
 *   get:
 *     summary: Get subscription analytics for a user
 *     tags:
 *       - Subscriptions
 */
router.get('/:userAddress/analytics', subscriptionController.getSubscriptionAnalytics);

// ─── Import/Export Routes (preserved) ──────────────────────────────────────────

router.post(
  '/import/validate',
  importRequestValidation,
  subscriptionController.validateImportRequest
);
router.post(
  '/import/preview',
  importRequestValidation,
  subscriptionController.previewImportRequest
);
router.post(
  '/import',
  importRequestValidation,
  subscriptionController.importSubscriptionsRequest
);
router.get(
  '/export/:merchantId',
  merchantParamValidation,
  subscriptionController.exportSubscriptionsRequest
);
router.get(
  '/import/history/:merchantId',
  merchantParamValidation,
  subscriptionController.getImportHistoryRequest
);

export default router;
