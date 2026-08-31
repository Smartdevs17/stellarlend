import { Router } from 'express';
import * as recoveryController from '../controllers/recovery.controller';

const router: Router = Router();

router.get('/guardians', recoveryController.getGuardians);
router.post('/guardians', recoveryController.setGuardians);
router.post('/guardians/add', recoveryController.addGuardian);
router.post('/guardians/remove', recoveryController.removeGuardian);
router.put('/threshold', recoveryController.setThreshold);
router.post('/start', recoveryController.startRecovery);
router.post('/approve', recoveryController.approveRecovery);
router.post('/execute', recoveryController.executeRecovery);
router.post('/cancel', recoveryController.cancelRecovery);
router.get('/request', recoveryController.getRecoveryRequest);
router.get('/approvals', recoveryController.getRecoveryApprovals);

export default router;
