import { Router } from 'express';
import * as reputationController from '../controllers/reputation.controller';

const router: Router = Router();

router.get('/analytics', reputationController.getAnalytics);
router.get('/deployer/:address', reputationController.getDeployerReputation);
router.get('/tiers', reputationController.getReputationTiers);
router.get('/leaderboard', reputationController.getLeaderboard);
router.get('/:address', reputationController.getReputation);

export default router;
