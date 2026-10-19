/**
 * Governance Parameter Store routes — Issue #697
 *
 * Mounted at `/api/parameters`.
 *
 * Route order matters here: the literal segments (`pools`, `proposals`,
 * `voting`, `simulate`, …) are declared before `/:pool`, otherwise a request
 * for `/proposals` would match the pool lookup.
 */

import { Router } from 'express';
import { parametersController } from '../controllers/parameters.controller';

const router: Router = Router();

/**
 * @openapi
 * /parameters/pools:
 *   post:
 *     summary: Register a pool with the parameter store
 *     tags:
 *       - Parameters
 *     requestBody:
 *       required: true
 *       content:
 *         application/json:
 *           schema:
 *             type: object
 *             required: [pool]
 *             properties:
 *               pool:
 *                 type: string
 *     responses:
 *       201:
 *         description: Pool registered
 *       400:
 *         description: Missing pool
 */
router.post('/pools', (req, res) => parametersController.registerPool(req, res));

/**
 * @openapi
 * /parameters/pools:
 *   get:
 *     summary: List registered pools
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Registered pool addresses
 */
router.get('/pools', (req, res) => parametersController.listPools(req, res));

/**
 * @openapi
 * /parameters/simulate:
 *   post:
 *     summary: Project the impact of a parameter change
 *     description: >
 *       Returns the projected change in borrowing power, the debt that would
 *       become liquidatable, the borrow-rate movement, a severity grade and any
 *       cross-parameter warnings. Mirrors the contract's `simulate_change`, so
 *       the numbers match what a voter sees on-chain.
 *     tags:
 *       - Parameters
 *     requestBody:
 *       required: true
 *       content:
 *         application/json:
 *           schema:
 *             type: object
 *             required: [pool, parameter, value]
 *             properties:
 *               pool:
 *                 type: string
 *               parameter:
 *                 type: string
 *                 enum: [LTV, LiquidationThreshold, CloseFactor, LiquidationIncentive, ReserveFactor, DebtCeiling, BaseInterestRate, Slope1, Slope2, OptimalUtilization]
 *               value:
 *                 type: number
 *               snapshot:
 *                 type: object
 *                 properties:
 *                   totalCollateral: { type: number }
 *                   totalDebt: { type: number }
 *                   totalDeposits: { type: number }
 *                   atRiskDebt: { type: number }
 *                   atRiskBandBps: { type: number }
 *     responses:
 *       200:
 *         description: Projected impact
 *       400:
 *         description: Invalid request
 */
router.post('/simulate', (req, res) => parametersController.simulate(req, res));

/**
 * @openapi
 * /parameters/validate:
 *   post:
 *     summary: Validate a value against its range and the pool's other parameters
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Validation result, with a reason when invalid
 *       400:
 *         description: Invalid request
 */
router.post('/validate', (req, res) => parametersController.validate(req, res));

/**
 * @openapi
 * /parameters/proposals:
 *   post:
 *     summary: Propose a parameter change
 *     description: >
 *       Risk parameters (LTV, liquidation threshold, close factor, liquidation
 *       incentive) carry a 48-hour minimum timelock; everything else 24 hours.
 *     tags:
 *       - Parameters
 *     responses:
 *       201:
 *         description: Proposal created
 *       400:
 *         description: Invalid value or timelock
 *       404:
 *         description: Pool not registered
 */
router.post('/proposals', (req, res) => parametersController.propose(req, res));

/**
 * @openapi
 * /parameters/proposals/emergency:
 *   post:
 *     summary: Propose an emergency risk-parameter change (4-hour timelock)
 *     tags:
 *       - Parameters
 *     responses:
 *       201:
 *         description: Emergency proposal created
 *       400:
 *         description: Not a risk parameter, or value out of range
 */
router.post('/proposals/emergency', (req, res) =>
  parametersController.proposeEmergency(req, res),
);

/**
 * @openapi
 * /parameters/proposals:
 *   get:
 *     summary: List proposals
 *     tags:
 *       - Parameters
 *     parameters:
 *       - in: query
 *         name: pool
 *         schema: { type: string }
 *       - in: query
 *         name: status
 *         schema:
 *           type: string
 *           enum: [pending, accepted, rejected]
 *     responses:
 *       200:
 *         description: Proposals, newest first
 */
router.get('/proposals', (req, res) => parametersController.listProposals(req, res));

/**
 * @openapi
 * /parameters/proposals/{id}:
 *   get:
 *     summary: Fetch one proposal
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: The proposal
 *       404:
 *         description: Not found
 */
router.get('/proposals/:id', (req, res) => parametersController.getProposal(req, res));

/**
 * @openapi
 * /parameters/proposals/{id}/accept:
 *   post:
 *     summary: Accept a proposal whose timelock has elapsed
 *     description: >
 *       When voting is enabled, the proposal must also have closed its voting
 *       window and passed both quorum and the approval threshold.
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Proposal accepted and the value written through
 *       400:
 *         description: Timelock not elapsed, voting open, or the vote failed
 */
router.post('/proposals/:id/accept', (req, res) => parametersController.accept(req, res));

/**
 * @openapi
 * /parameters/proposals/{id}/reject:
 *   post:
 *     summary: Reject a proposal
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Proposal rejected
 */
router.post('/proposals/:id/reject', (req, res) => parametersController.reject(req, res));

/**
 * @openapi
 * /parameters/proposals/{id}/votes:
 *   post:
 *     summary: Cast a vote on a proposal
 *     tags:
 *       - Parameters
 *     responses:
 *       201:
 *         description: Vote recorded
 *       400:
 *         description: Voting disabled or closed, no voting power, or already voted
 */
router.post('/proposals/:id/votes', (req, res) => parametersController.castVote(req, res));

/**
 * @openapi
 * /parameters/proposals/{id}/votes:
 *   get:
 *     summary: Votes cast on a proposal, with the current tally
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Votes and tally
 */
router.get('/proposals/:id/votes', (req, res) => parametersController.getVotes(req, res));

/**
 * @openapi
 * /parameters/voting/config:
 *   get:
 *     summary: Current voting rules
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Voting config, or `enabled: false` when voting is off
 */
router.get('/voting/config', (req, res) => parametersController.getVotingConfig(req, res));

/**
 * @openapi
 * /parameters/voting/config:
 *   put:
 *     summary: Install voting rules (governance-controlled)
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Updated config
 *       400:
 *         description: Invalid quorum, threshold or period
 */
router.put('/voting/config', (req, res) => parametersController.setVotingConfig(req, res));

/**
 * @openapi
 * /parameters/voting/power:
 *   put:
 *     summary: Set an address's voting weight (governance-controlled)
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Updated weight and total voting power
 */
router.put('/voting/power', (req, res) => parametersController.setVotingPower(req, res));

/**
 * @openapi
 * /parameters/notifications:
 *   get:
 *     summary: Parameter change notifications, newest first
 *     tags:
 *       - Parameters
 *     parameters:
 *       - in: query
 *         name: pool
 *         schema: { type: string }
 *       - in: query
 *         name: limit
 *         schema: { type: integer, minimum: 1, maximum: 500, default: 50 }
 *     responses:
 *       200:
 *         description: Change notifications
 */
router.get('/notifications', (req, res) => parametersController.getNotifications(req, res));

/**
 * @openapi
 * /parameters/{pool}:
 *   get:
 *     summary: All parameters and versions for a pool
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Values and versions
 */
router.get('/:pool', (req, res) => parametersController.getPoolParameters(req, res));

/**
 * @openapi
 * /parameters/{pool}/{parameter}:
 *   get:
 *     summary: One parameter's current value and version
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Value and version
 *       400:
 *         description: Unknown parameter
 */
router.get('/:pool/:parameter', (req, res) => parametersController.getParameter(req, res));

/**
 * @openapi
 * /parameters/{pool}/{parameter}/history:
 *   get:
 *     summary: Audit trail of a parameter's changes
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: Changes, oldest first, each carrying its version
 */
router.get('/:pool/:parameter/history', (req, res) =>
  parametersController.getHistory(req, res),
);

/**
 * @openapi
 * /parameters/{pool}/{parameter}/versions/{version}:
 *   get:
 *     summary: The value a parameter held at a given version
 *     tags:
 *       - Parameters
 *     responses:
 *       200:
 *         description: The historical value
 *       404:
 *         description: No such version
 */
router.get('/:pool/:parameter/versions/:version', (req, res) =>
  parametersController.getVersion(req, res),
);

export default router;
