import { Request, Response } from 'express';
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

export class EventsController {
  async getEvents(req: Request, res: Response): Promise<void> {
    try {
      const { type, address, from, to, limit = 50 } = req.query;
      const events = await getIndexedEvents({
        type: type as string | undefined,
        address: address as string | undefined,
        from: from ? Number(from) : undefined,
        to: to ? Number(to) : undefined,
        limit: Number(limit),
      });
      res.json(events);
    } catch (error) {
      res.status(500).json({ error: 'Failed to fetch events' });
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
      if (!schema) return res.status(404).json({ success: false, error: { message: `${name} not found` } });
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
