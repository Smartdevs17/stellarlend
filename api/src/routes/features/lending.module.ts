import { Router } from 'express';
import lendingRoutes from '../lending.routes';
import gasRoutes from '../gas.routes';
import debtTokenRoutes from '../debtToken.routes';
import crossAssetRoutes from '../crossAsset.routes';
import flashLoanRoutes from '../flashLoan.routes';

const router = Router();
router.use('/', lendingRoutes);
router.use('/gas', gasRoutes);
router.use('/debt-token', debtTokenRoutes);
router.use('/cross-asset', crossAssetRoutes);
router.use('/flash-loan', flashLoanRoutes);

export default router;
