import { Router } from 'express';
import { insuranceController } from '../controllers/insurance.controller';
import { authenticateToken } from '../middleware/auth';
import { requireRole } from '../middleware/rbac';

const router = Router();

// Public read and calculator routes
router.get('/policies', insuranceController.listPolicies);
router.get('/providers', insuranceController.listProviders);
router.get('/coverages', insuranceController.listCoverages);
router.get('/claims', insuranceController.getClaims);
router.post('/premium/calculate', insuranceController.calculatePremium);
router.get('/analytics', insuranceController.getAnalytics);

// Protected mutation routes
router.post('/providers', authenticateToken, insuranceController.onboardProvider);
router.patch('/providers/:id/kyc', authenticateToken, requireRole('admin'), insuranceController.updateKycStatus);
router.post('/policies', authenticateToken, insuranceController.createPolicy);
router.post('/coverages', authenticateToken, insuranceController.purchase);
router.post('/claims', authenticateToken, insuranceController.submitClaim);
router.post('/claims/:id/dispute', authenticateToken, insuranceController.disputeClaim);

export default router;
