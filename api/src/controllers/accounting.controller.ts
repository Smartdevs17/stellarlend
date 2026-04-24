import { Request, Response, NextFunction } from 'express';
import { AccountingService } from '../services/accounting.service';
import { logger } from '../utils/logger';

export class AccountingController {
  private accountingService: AccountingService;

  constructor() {
    this.accountingService = new AccountingService();
  }

  /**
   * Create a new subscription
   */
  createSubscription = async (req: Request, res: Response, next: NextFunction) => {
    try {
      const { merchantId, totalAmount, startTime } = req.body;

      logger.info('Creating subscription', { merchantId, totalAmount });

      const subscriptionId = await this.accountingService.createSubscription(
        merchantId,
        totalAmount,
        startTime
      );

      res.status(201).json({
        success: true,
        data: { subscriptionId },
      });
    } catch (error) {
      logger.error('Error creating subscription', { error });
      next(error);
    }
  };

  /**
   * Configure revenue recognition rule
   */
  configureRecognitionRule = async (req: Request, res: Response, next: NextFunction) => {
    try {
      const { subscriptionId } = req.params;
      const { method, recognitionPeriod } = req.body;

      logger.info('Configuring recognition rule', { subscriptionId, method });

      await this.accountingService.configureRecognitionRule(
        parseInt(subscriptionId),
        method,
        recognitionPeriod
      );

      res.json({
        success: true,
        message: 'Recognition rule configured successfully',
      });
    } catch (error) {
      logger.error('Error configuring recognition rule', { error });
      next(error);
    }
  };

  /**
   * Recognize revenue for a subscription
   */
  recognizeRevenue = async (req: Request, res: Response, next: NextFunction) => {
    try {
      const { subscriptionId } = req.params;

      logger.info('Recognizing revenue', { subscriptionId });

      const recognition = await this.accountingService.recognizeRevenue(
        parseInt(subscriptionId)
      );

      res.json({
        success: true,
        data: recognition,
      });
    } catch (error) {
      logger.error('Error recognizing revenue', { error });
      next(error);
    }
  };

  /**
   * Get deferred revenue for a merchant
   */
  getDeferredRevenue = async (req: Request, res: Response, next: NextFunction) => {
    try {
      const { merchantId } = req.params;

      logger.info('Getting deferred revenue', { merchantId });

      const deferredRevenue = await this.accountingService.getDeferredRevenue(merchantId);

      res.json({
        success: true,
        data: { deferredRevenue },
      });
    } catch (error) {
      logger.error('Error getting deferred revenue', { error });
      next(error);
    }
  };

  /**
   * Get revenue schedule for a subscription
   */
  getRevenueSchedule = async (req: Request, res: Response, next: NextFunction) => {
    try {
      const { subscriptionId } = req.params;

      logger.info('Getting revenue schedule', { subscriptionId });

      const schedule = await this.accountingService.getRevenueSchedule(
        parseInt(subscriptionId)
      );

      res.json({
        success: true,
        data: schedule,
      });
    } catch (error) {
      logger.error('Error getting revenue schedule', { error });
      next(error);
    }
  };

  /**
   * Get revenue analytics for a merchant
   */
  getRevenueAnalytics = async (req: Request, res: Response, next: NextFunction) => {
    try {
      const { merchantId } = req.params;
      const { startTime, endTime } = req.query;

      logger.info('Getting revenue analytics', { merchantId, startTime, endTime });

      const analytics = await this.accountingService.getRevenueAnalytics(
        merchantId,
        parseInt(startTime as string),
        parseInt(endTime as string)
      );

      res.json({
        success: true,
        data: analytics,
      });
    } catch (error) {
      logger.error('Error getting revenue analytics', { error });
      next(error);
    }
  };

  /**
   * Handle contract modification
   */
  modifyContract = async (req: Request, res: Response, next: NextFunction) => {
    try {
      const { subscriptionId } = req.params;
      const { newAmount } = req.body;

      logger.info('Modifying contract', { subscriptionId, newAmount });

      await this.accountingService.handleContractModification(
        parseInt(subscriptionId),
        newAmount
      );

      res.json({
        success: true,
        message: 'Contract modified successfully',
      });
    } catch (error) {
      logger.error('Error modifying contract', { error });
      next(error);
    }
  };

  /**
   * Handle subscription cancellation
   */
  cancelSubscription = async (req: Request, res: Response, next: NextFunction) => {
    try {
      const { subscriptionId } = req.params;

      logger.info('Cancelling subscription', { subscriptionId });

      const refundAmount = await this.accountingService.handleCancellation(
        parseInt(subscriptionId)
      );

      res.json({
        success: true,
        data: { refundAmount },
        message: 'Subscription cancelled successfully',
      });
    } catch (error) {
      logger.error('Error cancelling subscription', { error });
      next(error);
    }
  };

  /**
   * Get subscription state
   */
  getSubscriptionState = async (req: Request, res: Response, next: NextFunction) => {
    try {
      const { subscriptionId } = req.params;

      logger.info('Getting subscription state', { subscriptionId });

      const state = await this.accountingService.getSubscriptionState(
        parseInt(subscriptionId)
      );

      res.json({
        success: true,
        data: state,
      });
    } catch (error) {
      logger.error('Error getting subscription state', { error });
      next(error);
    }
  };

  /**
   * Get merchant subscriptions
   */
  getMerchantSubscriptions = async (req: Request, res: Response, next: NextFunction) => {
    try {
      const { merchantId } = req.params;

      logger.info('Getting merchant subscriptions', { merchantId });

      const subscriptions = await this.accountingService.getMerchantSubscriptions(merchantId);

      res.json({
        success: true,
        data: subscriptions,
      });
    } catch (error) {
      logger.error('Error getting merchant subscriptions', { error });
      next(error);
    }
  };
}
