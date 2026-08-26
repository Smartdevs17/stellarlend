import { Request, Response } from 'express';
import { getIndexedEvents, getEventTypes, getEventStats } from '../services/events.service';

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
