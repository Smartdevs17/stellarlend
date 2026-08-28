import { Router } from 'express';
import { insuranceController } from '../controllers/insurance.controller';

const router = Router();

router.get('/policies', insuranceController.listPolicies);
router.post('/providers', insuranceController.onboardProvider);
router.get('/providers', insuranceController.listProviders);
router.post('/policies', insuranceController.createPolicy);
router.post('/coverages', insuranceController.purchase);
router.get('/coverages', insuranceController.listCoverages);
router.post('/claims', insuranceController.submitClaim);
router.post('/claims/:id/dispute', insuranceController.disputeClaim);
router.get('/claims', insuranceController.getClaims);
router.post('/premium/calculate', insuranceController.calculatePremium);
router.get('/analytics', insuranceController.getAnalytics);

export default router;
