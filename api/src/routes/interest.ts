import { Router } from 'express';
import { interestController } from '../controllers/interest.controller';

const router: Router = Router();

/**
 * @openapi
 * /interest/current:
 *   get:
 *     summary: Current interest rates for the protocol
 *     description: Returns current borrow and supply rates in basis points
 *     tags:
 *       - Interest
 *     responses:
 *       200:
 *         description: Current interest rates
 */
router.get('/current', interestController.getCurrentRates);

/**
 * @openapi
 * /interest/history:
 *   get:
 *     summary: Historical interest rate snapshots
 *     description: Returns bounded history of rate snapshots
 *     tags:
 *       - Interest
 *     parameters:
 *       - in: query
 *         name: limit
 *         schema:
 *           type: integer
 *           default: 100
 */
router.get('/history', interestController.getRateHistory);

/**
 * @openapi
 * /interest/simulate:
 *   post:
 *     summary: Simulate borrow rate at a target utilization
 *     description: Returns simulated borrow rate without modifying state
 *     tags:
 *       - Interest
 *     requestBody:
 *       required: true
 *       content:
 *         application/json:
 *           schema:
 *             type: object
 *             properties:
 *               utilizationBps:
 *                 type: integer
 */
router.post('/simulate', interestController.simulateRate);

export default router;
