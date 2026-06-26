import { Router } from 'express';
import * as referralController from '../controllers/referral.controller';

const router: Router = Router();

router.post('/code', referralController.generateCode);
router.post('/register', referralController.registerReferral);
router.get('/stats/:userAddress', referralController.getStats);
router.post('/claim', referralController.claim);
router.get('/link/:userAddress', referralController.getReferralLink);

export default router;
