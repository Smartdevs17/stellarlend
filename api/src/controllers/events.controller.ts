import { Request, Response } from 'express';
import { NotFoundError } from '../utils/errors';
import * as eventsService from '../services/events.service';

/**
 * GET /events/schema
 * Full structured event schema catalog: version, vocabularies, event definitions.
 */
export const getSchema = (_req: Request, res: Response): void => {
  res.status(200).json(eventsService.getEventCatalog());
};

/**
 * GET /events/version
 * Just the current schema version, for cheap client compatibility checks.
 */
export const getSchemaVersion = (_req: Request, res: Response): void => {
  res.status(200).json({ schemaVersion: eventsService.EVENT_SCHEMA_VERSION });
};

/**
 * GET /events/modules
 * Canonical list of event module identifiers.
 */
export const getModules = (_req: Request, res: Response): void => {
  res.status(200).json({ modules: eventsService.listModules() });
};

/**
 * GET /events/actions
 * Canonical list of event action verbs.
 */
export const getActions = (_req: Request, res: Response): void => {
  res.status(200).json({ actions: eventsService.listActions() });
};

/**
 * GET /events/schema/:name
 * A single event definition. 404 if the name is unknown.
 */
export const getEventByName = (req: Request, res: Response): void => {
  const def = eventsService.getEventDefinition(req.params.name);
  if (!def) {
    throw new NotFoundError(`Unknown event '${req.params.name}'`);
  }
  res.status(200).json(def);
};
