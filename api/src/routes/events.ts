import { Router } from 'express';
import { eventsController } from '../controllers/events.controller';

const router: Router = Router();

/**
 * @openapi
 * /events:
 *   get:
 *     summary: Query indexed contract events with filters
 *     description: Returns paginated contract events filtered by type, address, or time range
 *     tags:
 *       - Events
 *     parameters:
 *       - in: query
 *         name: type
 *         schema:
 *           type: string
 *       - in: query
 *         name: address
 *         schema:
 *           type: string
 *       - in: query
 *         name: from
 *         schema:
 *           type: integer
 *       - in: query
 *         name: to
 *         schema:
 *           type: integer
 *       - in: query
 *         name: limit
 *         schema:
 *           type: integer
 *           default: 50
 */
router.get('/', eventsController.getEvents);
router.get('/schema', eventsController.getSchema);
router.get('/schema/:name', eventsController.getSchemaByName);
router.get('/version', eventsController.getVersion);
router.get('/modules', eventsController.getModules);
router.get('/actions', eventsController.getActions);

/**
 * @openapi
 * /events/types:
 *   get:
 *     summary: List all known event types
 *     description: Returns distinct event types available for filtering
 *     tags:
 *       - Events
 */
router.get('/types', eventsController.getEventTypes);

/**
 * @openapi
 * /events/stats:
 *   get:
 *     summary: Event volume statistics
 *     description: Returns counts and trends for recent events
 *     tags:
 *       - Events
 */
router.get('/stats', eventsController.getEventStats);

export default router;
