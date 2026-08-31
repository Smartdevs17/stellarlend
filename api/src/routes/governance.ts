import { Router } from 'express';
import * as governanceController from '../controllers/governance.controller';

const router: Router = Router();

router.get('/config', governanceController.getTimelockConfig);
router.post('/queue', governanceController.queueOperation);
router.post('/queue-batch', governanceController.queueBatchOperation);
router.post('/execute', governanceController.executeOperation);
router.post('/execute-batch', governanceController.executeBatchOperation);
router.post('/cancel', governanceController.cancelOperation);
router.get('/operations', governanceController.getPendingOperations);
router.get('/operations/:operationId', governanceController.getOperation);
router.get('/queue', governanceController.getQueue);
router.put('/config', governanceController.updateTimelockConfig);
router.put('/action-type-delay', governanceController.setActionTypeDelay);
router.get('/action-type-delay/:actionTypeId', governanceController.getActionTypeDelay);
router.post('/guardian/emergency-approve', governanceController.guardianApproveEmergency);
router.post('/guardian/emergency-execute', governanceController.guardianEmergencyExecute);
router.post('/clean-queue', governanceController.cleanQueue);

export default router;
