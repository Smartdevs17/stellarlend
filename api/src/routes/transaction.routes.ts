import { Router } from 'express';
import * as transactionController from '../controllers/transaction.controller';
import { authenticateToken } from '../middleware/auth';

const router: Router = Router();

router.use(authenticateToken);

router.post('/', transactionController.createTransaction);
router.get('/user/:userAddress', transactionController.listUserTransactions);
router.get('/:txId', transactionController.getTransaction);
router.post('/:txId/steps/:stepId/prepare', transactionController.prepareStep);
router.post('/steps/approve', transactionController.approveStep);
router.post('/steps/reject', transactionController.rejectStep);

export default router;
