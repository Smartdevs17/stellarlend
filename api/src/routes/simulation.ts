import { Router } from 'express';
import { simulationController } from '../controllers/simulation.controller';

const router: Router = Router();

// Position health simulation (price changes, deposits, withdrawals)
router.post('/position', (req, res, next) => simulationController.simulatePosition(req, res, next));

// Scenario modeling and stress testing
router.post('/scenario', (req, res, next) => simulationController.simulateScenario(req, res, next));

// What-if analysis and sensitivity recommendations
router.post('/what-if', (req, res, next) => simulationController.whatIfAnalysis(req, res, next));

// Simulation sharing
router.post('/share', (req, res, next) => simulationController.shareSimulation(req, res, next));
router.get('/share/:id', (req, res, next) => simulationController.getSharedSimulation(req, res, next));

// Comparison of simulation scenarios
router.post('/compare', (req, res, next) => simulationController.compareScenarios(req, res, next));

// Batch simulation
router.post('/batch', (req, res, next) => simulationController.batchSimulate(req, res, next));

export default router;
