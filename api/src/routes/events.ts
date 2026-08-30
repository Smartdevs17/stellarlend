import { Router } from 'express';
import * as eventsController from '../controllers/events.controller';

const router: Router = Router();

/**
 * @openapi
 * /events/schema:
 *   get:
 *     summary: Structured event schema catalog
 *     description: >
 *       Machine-readable mirror of the on-chain event schema
 *       (`stellar-lend/contracts/hello-world/src/events.rs`). Returns the current
 *       schema version, the canonical module/action vocabularies, and the topic
 *       layout + field types of every emitted event, including the versioned
 *       `structured_event_v1` envelope.
 *     tags:
 *       - Events
 *     responses:
 *       200:
 *         description: The full event catalog
 *         content:
 *           application/json:
 *             schema:
 *               type: object
 *               properties:
 *                 schemaVersion:
 *                   type: integer
 *                   example: 1
 *                 modules:
 *                   type: array
 *                   items:
 *                     type: string
 *                 actions:
 *                   type: array
 *                   items:
 *                     type: string
 *                 envelope:
 *                   type: string
 *                   example: structured_event_v1
 *                 events:
 *                   type: array
 *                   items:
 *                     type: object
 */
router.get('/schema', eventsController.getSchema);

/**
 * @openapi
 * /events/schema/{name}:
 *   get:
 *     summary: Single event definition
 *     tags:
 *       - Events
 *     parameters:
 *       - in: path
 *         name: name
 *         required: true
 *         schema:
 *           type: string
 *         description: Canonical event name, e.g. `structured_event_v1` or `deposit`
 *     responses:
 *       200:
 *         description: The event definition
 *       404:
 *         description: Unknown event name
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/ErrorResponse'
 */
router.get('/schema/:name', eventsController.getEventByName);

/**
 * @openapi
 * /events/version:
 *   get:
 *     summary: Current event schema version
 *     tags:
 *       - Events
 *     responses:
 *       200:
 *         description: The schema version
 *         content:
 *           application/json:
 *             schema:
 *               type: object
 *               properties:
 *                 schemaVersion:
 *                   type: integer
 *                   example: 1
 */
router.get('/version', eventsController.getSchemaVersion);

/**
 * @openapi
 * /events/modules:
 *   get:
 *     summary: Canonical event module identifiers
 *     tags:
 *       - Events
 *     responses:
 *       200:
 *         description: List of module identifiers
 */
router.get('/modules', eventsController.getModules);

/**
 * @openapi
 * /events/actions:
 *   get:
 *     summary: Canonical event action verbs
 *     tags:
 *       - Events
 *     responses:
 *       200:
 *         description: List of action verbs
 */
router.get('/actions', eventsController.getActions);

export default router;
