import { Router } from 'express';
import * as controller from '../controllers/pnl.controller';
import { authenticateToken } from '../middleware/auth';

const router: Router = Router();

router.post('/revenue', authenticateToken, controller.postRevenue);
router.post('/expense', authenticateToken, controller.postExpense);
router.get('/summary', controller.getSummary);
router.get('/breakdown', controller.getBreakdown);
router.get('/history', controller.getHistory);
router.get('/pools', controller.getRevenueByPool);
router.get('/cumulative', controller.getCumulativeChart);
router.get('/yield-vs-benchmark', controller.getYieldVsBenchmark);
router.get('/growth', controller.getRevenueGrowth);
router.get('/export', controller.getExport);

export default router;
