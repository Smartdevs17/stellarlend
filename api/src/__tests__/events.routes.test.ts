import request from 'supertest';
import app from '../app';
import {
  EVENT_SCHEMA_VERSION,
  EVENT_MODULES,
  EVENT_ACTIONS,
} from '../services/events.service';

describe('Event schema routes (/api/events)', () => {
  describe('GET /api/events/schema', () => {
    it('returns the full catalog with version, vocabularies and events', async () => {
      const res = await request(app).get('/api/events/schema');

      expect(res.status).toBe(200);
      expect(res.body.schemaVersion).toBe(EVENT_SCHEMA_VERSION);
      expect(res.body.envelope).toBe('structured_event_v1');
      expect(res.body.modules).toEqual([...EVENT_MODULES]);
      expect(res.body.actions).toEqual([...EVENT_ACTIONS]);
      expect(Array.isArray(res.body.events)).toBe(true);
      expect(res.body.events.length).toBeGreaterThan(1);
    });

    it('includes the structured_event_v1 envelope with its documented topic layout', async () => {
      const res = await request(app).get('/api/events/schema');
      const envelope = res.body.events.find(
        (e: { name: string }) => e.name === 'structured_event_v1'
      );

      expect(envelope).toBeDefined();
      expect(envelope.topicPrefix).toBe('proto_evt');
      expect(envelope.action).toBeNull();

      const topicFields = envelope.fields
        .filter((f: { topic: boolean }) => f.topic)
        .map((f: { name: string }) => f.name);
      expect(topicFields).toEqual(['module', 'action', 'actor']);

      const schemaVersionField = envelope.fields.find(
        (f: { name: string }) => f.name === 'schema_version'
      );
      expect(schemaVersionField.topic).toBe(false);
      expect(schemaVersionField.type).toBe('u32');
    });

    it('gives every event a known module and (nullable) known action', async () => {
      const res = await request(app).get('/api/events/schema');

      for (const event of res.body.events) {
        expect(EVENT_MODULES).toContain(event.module);
        if (event.action !== null) {
          expect(EVENT_ACTIONS).toContain(event.action);
        }
        expect(event.fields.length).toBeGreaterThan(0);
      }
    });
  });

  describe('GET /api/events/schema/:name', () => {
    it('returns a single typed event definition', async () => {
      const res = await request(app).get('/api/events/schema/liquidation');

      expect(res.status).toBe(200);
      expect(res.body.name).toBe('liquidation');
      expect(res.body.module).toBe('liquidation');
      expect(res.body.action).toBe('liquidate');
      const topicFields = res.body.fields
        .filter((f: { topic: boolean }) => f.topic)
        .map((f: { name: string }) => f.name);
      expect(topicFields).toEqual(['liquidator', 'borrower']);
    });

    it('returns 404 with an error body for an unknown event name', async () => {
      const res = await request(app).get('/api/events/schema/not-a-real-event');

      expect(res.status).toBe(404);
      expect(res.body.success).toBe(false);
      expect(res.body.error.message).toContain('not-a-real-event');
    });
  });

  describe('GET /api/events/version', () => {
    it('returns just the schema version', async () => {
      const res = await request(app).get('/api/events/version');

      expect(res.status).toBe(200);
      expect(res.body).toEqual({ schemaVersion: EVENT_SCHEMA_VERSION });
    });
  });

  describe('GET /api/events/modules', () => {
    it('returns the canonical module list', async () => {
      const res = await request(app).get('/api/events/modules');

      expect(res.status).toBe(200);
      expect(res.body.modules).toEqual([...EVENT_MODULES]);
      expect(res.body.modules).toContain('lending');
    });
  });

  describe('GET /api/events/actions', () => {
    it('returns the canonical action list', async () => {
      const res = await request(app).get('/api/events/actions');

      expect(res.status).toBe(200);
      expect(res.body.actions).toEqual([...EVENT_ACTIONS]);
      expect(res.body.actions).toContain('deposit');
      expect(res.body.actions).toContain('other');
    });
  });
});
