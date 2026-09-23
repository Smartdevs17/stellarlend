import { NextFunction, Request, Response } from 'express';
import {
  getIndexedEvents,
  getEventTypes,
  getEventStats,
  getEventSchemaCatalog,
  getEventSchemaByName,
  getEventSchemaVersion,
  getEventModules,
  getEventActions,
} from '../services/events.service';
import { getEventIndexer } from '../services/eventIndex';
import type { EventQuery } from '../services/eventIndex/types';
import type { ReplaySource } from '../services/eventIndex/replay';
import { NotFoundError, ValidationError } from '../utils/errors';

const DAY_MS = 24 * 60 * 60 * 1000;

type Params = Record<string, unknown>;

function str(params: Params, key: string): string | undefined {
  const v = params[key];
  if (v === undefined || v === null || v === '') return undefined;
  if (typeof v !== 'string' && typeof v !== 'number') {
    throw new ValidationError(`${key} must be a string`);
  }
  return String(v);
}

function int(params: Params, key: string, min = 0): number | undefined {
  const v = str(params, key);
  if (v === undefined) return undefined;
  const n = Number(v);
  if (!Number.isInteger(n) || n < min)
    throw new ValidationError(`${key} must be an integer >= ${min}`);
  return n;
}

/** Epoch milliseconds or an ISO-8601 date. */
function time(params: Params, key: string): number | undefined {
  const v = str(params, key);
  if (v === undefined) return undefined;
  const n = /^\d+$/.test(v) ? Number(v) : Date.parse(v);
  if (!Number.isFinite(n))
    throw new ValidationError(`${key} must be epoch milliseconds or an ISO date`);
  return n;
}

function bool(params: Params, key: string): boolean | undefined {
  const v = params[key];
  if (v === undefined || v === '') return undefined;
  if (v === true || v === 'true') return true;
  if (v === false || v === 'false') return false;
  throw new ValidationError(`${key} must be true or false`);
}

function oneOf<T extends string>(
  params: Params,
  key: string,
  allowed: readonly T[]
): T | undefined {
  const v = str(params, key);
  if (v === undefined) return undefined;
  if (!allowed.includes(v as T))
    throw new ValidationError(`${key} must be one of: ${allowed.join(', ')}`);
  return v as T;
}

/** Parse the shared filter set used by query, replay and analytics. */
export function parseEventQuery(params: Params): EventQuery {
  const query: EventQuery = {
    type: str(params, 'type'),
    // `address` is the pre-#685 name of the account filter.
    account: str(params, 'account') ?? str(params, 'address'),
    contract: str(params, 'contract'),
    module: str(params, 'module'),
    action: str(params, 'action'),
    from: time(params, 'from'),
    to: time(params, 'to'),
    fromLedger: int(params, 'fromLedger'),
    toLedger: int(params, 'toLedger'),
    order: oneOf(params, 'order', ['asc', 'desc'] as const),
    limit: int(params, 'limit', 1),
    cursor: str(params, 'cursor'),
    includeArchived: bool(params, 'includeArchived'),
  };
  if (query.from !== undefined && query.to !== undefined && query.from > query.to) {
    throw new ValidationError('from must be <= to');
  }
  if (
    query.fromLedger !== undefined &&
    query.toLedger !== undefined &&
    query.fromLedger > query.toLedger
  ) {
    throw new ValidationError('fromLedger must be <= toLedger');
  }
  return query;
}

export class EventsController {
  async getEvents(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      res.json(await getIndexedEvents(parseEventQuery(req.query as Params)));
    } catch (error) {
      next(error);
    }
  }

  async getEventById(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const id = req.params.id ?? '';
      const event = getEventIndexer().get(id);
      if (!event) throw new NotFoundError(`event ${id} not found`);
      res.json(event);
    } catch (error) {
      next(error);
    }
  }

  async getAnalytics(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const params = req.query as Params;
      res.json(
        getEventIndexer().analytics({
          from: time(params, 'from'),
          to: time(params, 'to'),
          bucket: oneOf(params, 'bucket', ['hour', 'day'] as const),
          top: int(params, 'top', 1),
        })
      );
    } catch (error) {
      next(error);
    }
  }

  async getIndexerStatus(_req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      res.json(await getEventIndexer().status());
    } catch (error) {
      next(error);
    }
  }

  async syncIndexer(_req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      res.json(await getEventIndexer().sync());
    } catch (error) {
      next(error);
    }
  }

  async archive(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const olderThanDays = int((req.body ?? {}) as Params, 'olderThanDays', 1);
      const result = await getEventIndexer().archive(
        olderThanDays !== undefined ? olderThanDays * DAY_MS : undefined
      );
      res.json(result);
    } catch (error) {
      next(error);
    }
  }

  async getArchiveSegments(_req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      res.json(await getEventIndexer().archiveManifest());
    } catch (error) {
      next(error);
    }
  }

  async replay(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const body = (req.body ?? {}) as Params;
      const query = parseEventQuery(body);
      const result = await getEventIndexer().replay({
        ...query,
        includeArchived: query.includeArchived ?? true,
        source: oneOf(body, 'source', ['typed', 'structured'] as const) as ReplaySource | undefined,
        maxTransitions: int(body, 'maxTransitions', 0),
      });
      res.json(result);
    } catch (error) {
      next(error);
    }
  }

  async getEventTypes(_req: Request, res: Response): Promise<void> {
    try {
      const types = getEventTypes();
      res.json(types);
    } catch (error) {
      res.status(500).json({ error: 'Failed to fetch event types' });
    }
  }

  async getSchema(_req: Request, res: Response): Promise<void> {
    try {
      const catalog = getEventSchemaCatalog();
      res.json(catalog);
    } catch (error) {
      res.status(500).json({ error: 'Failed to fetch event schema' });
    }
  }

  async getSchemaByName(req: Request, res: Response): Promise<void> {
    try {
      const { name } = req.params;
      const schema = getEventSchemaByName(name);
      if (!schema)
        return res.status(404).json({ success: false, error: { message: `${name} not found` } });
      res.json(schema);
    } catch (error) {
      res.status(500).json({ error: 'Failed to fetch event schema' });
    }
  }

  async getVersion(_req: Request, res: Response): Promise<void> {
    try {
      res.json(getEventSchemaVersion());
    } catch (error) {
      res.status(500).json({ error: 'Failed to fetch schema version' });
    }
  }

  async getModules(_req: Request, res: Response): Promise<void> {
    try {
      res.json(getEventModules());
    } catch (error) {
      res.status(500).json({ error: 'Failed to fetch modules' });
    }
  }

  async getActions(_req: Request, res: Response): Promise<void> {
    try {
      res.json(getEventActions());
    } catch (error) {
      res.status(500).json({ error: 'Failed to fetch actions' });
    }
  }

  async getEventStats(_req: Request, res: Response): Promise<void> {
    try {
      const stats = getEventStats();
      res.json(stats);
    } catch (error) {
      res.status(500).json({ error: 'Failed to fetch event stats' });
    }
  }
}

export const eventsController = new EventsController();
