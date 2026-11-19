import { Router } from 'express';
import * as controller from '../controllers/poolPerformance.controller';
import * as mvController from '../controllers/poolUtilizationMv.controller';

const router: Router = Router();

router.get('/snapshots/:poolAddress', controller.getPoolSnapshots);
router.get('/metrics/:poolAddress', controller.getPoolMetrics);
router.get('/compare', controller.comparePools);
router.get('/export/:poolAddress', controller.exportPerformanceData);
router.get('/summary', controller.getPerformanceSummary);
router.get('/charts/:poolAddress', controller.getChartSeries);
router.get('/heatmap/:poolAddress', controller.getUtilizationHeatmap);
router.get('/benchmarks/:poolAddress', controller.getBenchmarks);
router.get('/events', controller.getPerformanceEvents);
router.get('/apr-apy-calculator', controller.calculateAprApy);
router.get('/returns/:poolAddress', controller.getHistoricalReturns);
router.post('/snapshots', controller.captureSnapshot);

// Issue #1650 — materialised view for pool-utilization time series
// GET  /utilization-timeseries                 → all pools
// GET  /utilization-timeseries/:poolAddress    → single pool
// POST /utilization-timeseries/refresh         → manual view refresh (admin)
router.get('/utilization-timeseries', mvController.getAllUtilizationTimeSeries);
router.get('/utilization-timeseries/:poolAddress', mvController.getUtilizationTimeSeries);
router.post('/utilization-timeseries/refresh', mvController.manualRefreshUtilizationTimeSeries);

export default router;
