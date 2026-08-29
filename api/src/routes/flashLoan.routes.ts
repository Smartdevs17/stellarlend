import { Router } from 'express';
import * as controller from '../controllers/flashLoanLiquidation.controller';

const router: Router = Router();

router.post('/simulate', controller.simulateCombo);
router.post('/execute', controller.executeCombo);
router.post('/multi-asset', controller.simulateMultiAsset);

router.get('/metrics/:asset', (req, res) => {
  res.json({ message: 'Flash loan metrics for asset ' + req.params.asset });
});

export default router;
