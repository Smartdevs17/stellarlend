/**
 * Issue #1373 – Generate the published OpenAPI document from source.
 *
 * Usage: npm run openapi:generate   (writes oracle/openapi.json)
 *        npm run openapi:check      (CI: fails if the committed spec drifts)
 */

import { writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildOpenApiDocument } from '../src/api/openapi.js';

const here = dirname(fileURLToPath(import.meta.url));
const outputPath = resolve(here, '..', 'openapi.json');

const document = buildOpenApiDocument({
  version: process.env.npm_package_version ?? '1.0.0',
});

writeFileSync(outputPath, `${JSON.stringify(document, null, 2)}\n`);
// eslint-disable-next-line no-console
console.log(`OpenAPI spec written to ${outputPath}`);
