import { Router } from 'express';
import * as lendingController from '../controllers/lending.controller';
import { prepareValidation, submitValidation } from '../middleware/validation';

const router = Router();

/**
 * @openapi
 * /lending/prepare/{operation}:
 *   get:
 *     summary: Prepare an unsigned lending transaction
 *     description: Builds an unsigned Soroban transaction XDR for the given lending operation. The client must sign the XDR and submit it via the submit endpoint.
 *     tags:
 *       - Lending
 *     parameters:
 *       - in: path
 *         name: operation
 *         required: true
 *         schema:
 *           type: string
 *           enum: [deposit, borrow, repay, withdraw]
 *         description: The lending operation to prepare
 *       - in: query
 *         name: userAddress
 *         required: true
 *         schema:
 *           type: string
 *         description: Stellar public key (Ed25519)
 *       - in: query
 *         name: amount
 *         required: true
 *         schema:
 *           type: string
 *         description: Amount as a positive integer string (stroops)
 *       - in: query
 *         name: assetAddress
 *         required: false
 *         schema:
 *           type: string
 *         description: Optional asset contract address
 *     responses:
 *       200:
 *         description: Unsigned transaction XDR ready for signing
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/PrepareResponse'
 *       400:
 *         description: Validation error
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 *       500:
 *         description: Internal server error
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 */
// v2: client-side signing flow
router.get('/prepare/:operation', prepareValidation, lendingController.prepare);

/**
 * @openapi
 * /lending/submit:
 *   post:
 *     summary: Submit a signed transaction
 *     description: Submits a client-signed transaction XDR to the Stellar network and monitors it until completion.
 *     tags:
 *       - Lending
 *     requestBody:
 *       required: true
 *       content:
 *         application/json:
 *           schema:
 *             $ref: '#/components/schemas/SubmitRequest'
 *     responses:
 *       200:
 *         description: Transaction submitted and monitored successfully
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/TransactionResponse'
 *       400:
 *         description: Validation error or transaction failed
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 *       500:
 *         description: Internal server error
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 */
router.post('/submit', submitValidation, lendingController.submit);

export default router;
