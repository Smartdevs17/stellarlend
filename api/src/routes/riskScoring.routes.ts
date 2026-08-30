import { Router } from 'express';
import { riskScoringController } from '../controllers/riskScoring.controller';

const router = Router();

router.get('/scores', riskScoringController.getAllScores);
router.get('/scores/:pool', riskScoringController.getPoolScore);
router.get('/profile/:pool', riskScoringController.getPoolProfile);
router.get('/distribution', riskScoringController.getDistribution);
router.get('/weights', riskScoringController.getWeights);
router.get('/alerts', riskScoringController.getAlerts);
router.post('/alerts/:alertId/acknowledge', riskScoringController.acknowledgeAlert);
router.get('/analytics', riskScoringController.getAnalytics);

export default router;