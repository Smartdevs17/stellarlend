import { Router, Request, Response } from 'express';
import { budgetPlanner } from '../services/planner/budget-planner';

const router = Router();

/**
 * @route POST /api/planner/budget
 * @desc Build budget plan with allocations and steps
 */
router.post('/budget', (req: Request, res: Response) => {
  try {
    res.json({ success: true, data: budgetPlanner.build(req.body) });
  } catch (error) {
    res
      .status(400)
      .json({ success: false, error: error instanceof Error ? error.message : 'Invalid plan' });
  }
});

/**
 * @route POST /api/planner/yield-projections
 * @desc Calculate yield projections with compounding and multiple horizons
 */
router.post('/yield-projections', (req: Request, res: Response) => {
  try {
    const result = budgetPlanner.calculateYieldProjections(req.body);
    res.json({ success: true, data: result });
  } catch (error) {
    res
      .status(400)
      .json({ success: false, error: error instanceof Error ? error.message : 'Calculation error' });
  }
});

/**
 * @route POST /api/planner/risk-assessment
 * @desc Assess portfolio risk, diversification, and stress drawdown
 */
router.post('/risk-assessment', (req: Request, res: Response) => {
  try {
    const { pools, capital } = req.body;
    if (!pools || !capital) {
      return res.status(400).json({ success: false, error: 'pools and capital required' });
    }
    const result = budgetPlanner.assessRisk(pools, capital);
    res.json({ success: true, data: result });
  } catch (error) {
    res
      .status(400)
      .json({ success: false, error: error instanceof Error ? error.message : 'Assessment error' });
  }
});

/**
 * @route POST /api/planner/optimize
 * @desc Optimize budget allocations using max_yield, min_risk, or balanced strategy
 */
router.post('/optimize', (req: Request, res: Response) => {
  try {
    const result = budgetPlanner.optimize(req.body);
    res.json({ success: true, data: result });
  } catch (error) {
    res
      .status(400)
      .json({ success: false, error: error instanceof Error ? error.message : 'Optimization error' });
  }
});

/**
 * @route POST /api/planner/plans
 * @desc Save a new budget plan for tracking
 */
router.post('/plans', (req: Request, res: Response) => {
  try {
    const plan = budgetPlanner.createTrackedPlan(req.body);
    res.status(201).json({ success: true, data: plan });
  } catch (error) {
    res
      .status(400)
      .json({ success: false, error: error instanceof Error ? error.message : 'Failed to save plan' });
  }
});

/**
 * @route GET /api/planner/plans
 * @desc List saved plans for a user
 */
router.get('/plans', (req: Request, res: Response) => {
  try {
    const userAddress = (req.query.userAddress as string) || 'default';
    const plans = budgetPlanner.listTrackedPlans(userAddress);
    res.json({ success: true, data: plans });
  } catch (error) {
    res
      .status(500)
      .json({ success: false, error: error instanceof Error ? error.message : 'Failed to fetch plans' });
  }
});

/**
 * @route GET /api/planner/plans/:id
 * @desc Get details of a tracked plan
 */
router.get('/plans/:id', (req: Request, res: Response) => {
  try {
    const plan = budgetPlanner.getTrackedPlan(req.params.id);
    if (!plan) {
      return res.status(404).json({ success: false, error: 'Plan not found' });
    }
    res.json({ success: true, data: plan });
  } catch (error) {
    res
      .status(500)
      .json({ success: false, error: error instanceof Error ? error.message : 'Failed to fetch plan' });
  }
});

/**
 * @route POST /api/planner/plans/:id/record-actual
 * @desc Record actual performance returns to track variance
 */
router.post('/plans/:id/record-actual', (req: Request, res: Response) => {
  try {
    const { actualReturn } = req.body;
    if (actualReturn === undefined) {
      return res.status(400).json({ success: false, error: 'actualReturn is required' });
    }
    const updated = budgetPlanner.recordActualPerformance(req.params.id, Number(actualReturn));
    res.json({ success: true, data: updated });
  } catch (error) {
    res
      .status(400)
      .json({ success: false, error: error instanceof Error ? error.message : 'Update failed' });
  }
});

/**
 * @route POST /api/planner/alerts
 * @desc Configure budget alert
 */
router.post('/alerts', (req: Request, res: Response) => {
  try {
    const { userAddress, alert } = req.body;
    if (!userAddress || !alert) {
      return res.status(400).json({ success: false, error: 'userAddress and alert required' });
    }
    const created = budgetPlanner.configureAlert(userAddress, alert);
    res.status(201).json({ success: true, data: created });
  } catch (error) {
    res
      .status(400)
      .json({ success: false, error: error instanceof Error ? error.message : 'Alert creation failed' });
  }
});

/**
 * @route GET /api/planner/alerts
 * @desc Get alerts for user
 */
router.get('/alerts', (req: Request, res: Response) => {
  try {
    const userAddress = (req.query.userAddress as string) || 'default';
    const alerts = budgetPlanner.getAlerts(userAddress);
    res.json({ success: true, data: alerts });
  } catch (error) {
    res
      .status(500)
      .json({ success: false, error: error instanceof Error ? error.message : 'Failed to get alerts' });
  }
});

/**
 * @route POST /api/planner/alerts/evaluate
 * @desc Evaluate alerts against current pool data
 */
router.post('/alerts/evaluate', (req: Request, res: Response) => {
  try {
    const { userAddress, currentPools } = req.body;
    if (!userAddress || !currentPools) {
      return res.status(400).json({ success: false, error: 'userAddress and currentPools required' });
    }
    const triggered = budgetPlanner.evaluateAlerts(userAddress, currentPools);
    res.json({ success: true, data: triggered });
  } catch (error) {
    res
      .status(400)
      .json({ success: false, error: error instanceof Error ? error.message : 'Evaluation failed' });
  }
});

export default router;
