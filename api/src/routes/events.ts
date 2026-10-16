import { Router } from 'express';
import { eventsController } from '../controllers/events.controller';
import { requireRole } from '../middleware/rbac';

const router: Router = Router();

/**
 * @openapi
 * /events:
 *   get:
 *     summary: Query indexed contract events with filters
 *     description: >
 *       Returns a page of indexed contract events, newest first by default.
 *       Filters combine with AND. Pass `nextCursor` back as `cursor` for the
 *       next page. Set `includeArchived=true` to also search cold storage.
 *     tags:
 *       - Events
 *     parameters:
 *       - { in: query, name: type, schema: { type: string }, description: 'Event type, e.g. deposit, liquidation, proto_evt' }
 *       - { in: query, name: account, schema: { type: string }, description: 'Any address referenced by the event (alias: address)' }
 *       - { in: query, name: contract, schema: { type: string } }
 *       - { in: query, name: module, schema: { type: string }, description: 'Structured envelope module, e.g. lending' }
 *       - { in: query, name: action, schema: { type: string }, description: 'Structured envelope action, e.g. borrow' }
 *       - { in: query, name: from, schema: { type: string }, description: 'Epoch ms or ISO date (inclusive)' }
 *       - { in: query, name: to, schema: { type: string }, description: 'Epoch ms or ISO date (inclusive)' }
 *       - { in: query, name: fromLedger, schema: { type: integer } }
 *       - { in: query, name: toLedger, schema: { type: integer } }
 *       - { in: query, name: order, schema: { type: string, enum: [asc, desc], default: desc } }
 *       - { in: query, name: limit, schema: { type: integer, default: 50, maximum: 1000 } }
 *       - { in: query, name: cursor, schema: { type: string } }
 *       - { in: query, name: includeArchived, schema: { type: boolean, default: false } }
 *     responses:
 *       200:
 *         description: '{ events, nextCursor, matched, tookMs, source }'
 *       400:
 *         description: Invalid filter
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
 *     description: Returns total and per-type counts of indexed events
 *     tags:
 *       - Events
 */
router.get('/stats', eventsController.getEventStats);

/**
 * @openapi
 * /events/analytics:
 *   get:
 *     summary: Event analytics for dashboards
 *     description: >
 *       Counts by type / module / schema status, a time series, top accounts,
 *       volume per asset for amount-bearing events, and recent query latency.
 *     tags:
 *       - Events
 *     parameters:
 *       - { in: query, name: from, schema: { type: string } }
 *       - { in: query, name: to, schema: { type: string } }
 *       - { in: query, name: bucket, schema: { type: string, enum: [hour, day], default: hour } }
 *       - { in: query, name: top, schema: { type: integer, default: 10 } }
 */
router.get('/analytics', eventsController.getAnalytics);

/**
 * @openapi
 * /events/indexer/status:
 *   get:
 *     summary: Indexer health — cursor, lag, archive summary, sink failures
 *     tags:
 *       - Events
 */
router.get('/indexer/status', eventsController.getIndexerStatus);

/**
 * @openapi
 * /events/indexer/sync:
 *   post:
 *     summary: Pull new events from Soroban RPC now (operator)
 *     tags:
 *       - Events
 */
router.post('/indexer/sync', requireRole('operator'), eventsController.syncIndexer);

/**
 * @openapi
 * /events/archive:
 *   get:
 *     summary: Archive manifest — cold-storage segments with ledger/time bounds
 *     tags:
 *       - Events
 *   post:
 *     summary: Move events older than the retention window to cold storage (operator)
 *     tags:
 *       - Events
 *     requestBody:
 *       content:
 *         application/json:
 *           schema:
 *             type: object
 *             properties:
 *               olderThanDays: { type: integer, minimum: 1, description: 'Defaults to EVENT_HOT_RETENTION_DAYS' }
 */
router.get('/archive', eventsController.getArchiveSegments);
router.post('/archive', requireRole('operator'), eventsController.archive);

/**
 * @openapi
 * /events/replay:
 *   post:
 *     summary: Replay events to reconstruct account state (operator, debugging)
 *     description: >
 *       Re-applies events in ledger order (hot + archived) and returns per-account
 *       collateral/debt, every state transition, and anomalies such as balances
 *       going negative. Accepts the same filters as GET /events plus
 *       `source` (typed | structured) and `maxTransitions`.
 *     tags:
 *       - Events
 */
router.post('/replay', requireRole('operator'), eventsController.replay);

/**
 * @openapi
 * /events/{id}:
 *   get:
 *     summary: Fetch a single indexed event by RPC event id
 *     tags:
 *       - Events
 */
router.get('/:id', eventsController.getEventById);

export default router;
