/**
 * Issue #1373 – The published OpenAPI document must be generated from source
 * and never drift.
 */

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildOpenApiDocument, ROUTES } from '../src/api/openapi.js';

const here = dirname(fileURLToPath(import.meta.url));
const specPath = resolve(here, '..', 'openapi.json');

describe('OpenAPI spec generation', () => {
  it('documents every query route exactly once', () => {
    const document = buildOpenApiDocument();
    const paths = Object.keys(document.paths);

    expect(paths).toHaveLength(ROUTES.length);
    for (const route of ROUTES) {
      expect(paths).toContain(route.path);
      expect((document.paths as Record<string, any>)[route.path].get).toBeDefined();
    }
  });

  it('documents pagination parameters and the capped limit', () => {
    const document = buildOpenApiDocument();
    const history = (document.paths as Record<string, any>)['/prices/{asset}/history'].get;
    const params = Object.fromEntries(
      (history.parameters as Array<{ name: string; schema: Record<string, unknown> }>).map(
        (param) => [param.name, param.schema]
      )
    );

    expect(params.limit).toMatchObject({ maximum: 100, default: 20 });
    expect(params.cursor).toBeDefined();
    expect(params.from).toBeDefined();
    expect(params.to).toBeDefined();
  });

  it('the committed oracle/openapi.json matches generated output (no drift)', () => {
    const committed = JSON.parse(readFileSync(specPath, 'utf8'));
    expect(committed).toEqual(buildOpenApiDocument({ version: '1.0.0' }));
  });
});
