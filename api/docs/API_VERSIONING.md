# StellarLend API Versioning

This document describes the StellarLend REST API versioning scheme, deprecation
policy, and migration path for clients.

## Overview

The StellarLend API is versioned using URL path segments and response headers.
Each major version is deployed under its own base path (e.g. `/api/v1`) and
advertises itself through the `X-API-Version` response header.

## Supported versions

| Version | Base path   | Status     | Deprecated in | Sunset date | Migrate to |
|---------|-------------|------------|---------------|-------------|------------|
| v1      | `/api/v1`   | Current    | —             | —           | —          |

The legacy unversioned `/api/*` paths are preserved for backward compatibility
and serve the v1 specification with deprecation headers attached.

## Version discovery

Clients can enumerate all available API versions:

```
GET /api/versions
```

Response:

```json
{
  "versions": [
    { "version": "v1", "deprecated": false }
  ],
  "current": "v1"
}
```

OpenAPI specifications are published per version:

```
GET /api/v1/openapi.json   # v1 spec
GET /api/openapi.json      # legacy alias (v1 spec, X-API-Deprecated: true)
```

## Version headers

Every response from a versioned route includes:

| Header | Meaning |
|--------|---------|
| `X-API-Version` | The API version serving the request (e.g. `v1`) |

Legacy/deprecated routes additionally include:

| Header | Meaning |
|--------|---------|
| `X-API-Deprecated` | `true` when the route is deprecated |
| `X-API-Deprecated-In` | The version in which deprecation was announced |
| `X-API-Sunset` | ISO date after which the version may be removed |
| `X-API-Migrate-To` | Target path clients should migrate to |

Deprecation metadata is also injected in-band into JSON response bodies under
the `deprecation` key, with a `_links.latestVersion` link for machine-readable
migration.

Example deprecated response:

```http
X-API-Deprecated: true
X-API-Deprecated-In: v2
X-API-Sunset: 2027-06-30
X-API-Migrate-To: /api/v2/lending
```

```json
{
  "deprecated": true,
  "deprecatedIn": "v2",
  "sunset": "2027-06-30",
  "migrateTo": "/api/v2/lending",
  "_links": { "latestVersion": { "href": "/api/v2/lending" } }
}
```

## Deprecation policy

1. A version is announced as deprecated at least one major release cycle before
   its sunset date.
2. Deprecation is communicated via response headers, in-band body metadata, and
   the `/api/versions` endpoint.
3. Clients receive a `sunset` date indicating when the deprecated version may be
   removed.
4. Breaking changes are only introduced in new major versions; minor versions
   are additive.

## Client recommendations

- Pin to a specific version path (e.g. `/api/v1`) rather than the legacy
  unversioned alias.
- Read `X-API-Version` on responses to confirm which version served the request.
- Monitor `X-API-Deprecated` and `X-API-Sunset` to plan migrations.
- Follow `X-API-Migrate-To` / `_links.latestVersion` when a deprecation is
  announced.

## Adding a new version

1. Create route modules under `src/routes/v<N>/`.
2. Add a `VersionedOpenApiConfig` entry in `src/config/swagger.ts`.
3. Mount the routes in `src/app.ts` with `versionMiddleware({ version: 'v<N>' })`.
4. Deprecate the prior version using `legacyCompatibilityMiddleware` and publish
   a sunset date.

## Related

- Issue #1419: Add API versioning
- Middleware: `api/src/middleware/versioning.ts`
- Spec builder: `api/src/config/swagger.ts`
