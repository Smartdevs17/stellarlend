import { Router } from 'express';
import { yieldAggregatorController } from '../controllers/yield-aggregator.controller';

const router: Router = Router();

// Pool yield aggregation
router.get('/pools', (req, res, next) => yieldAggregatorController.getPools(req, res, next));

// Best-rate routing algorithm
router.post('/route', (req, res, next) => yieldAggregatorController.getBestRateRoute(req, res, next));

// Yield comparison tools
router.post('/compare', (req, res, next) => yieldAggregatorController.comparePools(req, res, next));

// Yield analytics & historical curves
router.get('/analytics/:poolId', (req, res, next) => yieldAggregatorController.getPoolAnalytics(req, res, next));

// Yield alerts management
router.post('/alerts', (req, res, next) => yieldAggregatorController.createAlert(req, res, next));
router.get('/alerts/check', (req, res, next) => yieldAggregatorController.checkAlerts(req, res, next));
router.get('/alerts/:userId', (req, res, next) => yieldAggregatorController.getUserAlerts(req, res, next));
router.delete('/alerts/:id', (req, res, next) => yieldAggregatorController.deleteAlert(req, res, next));

export default router;
