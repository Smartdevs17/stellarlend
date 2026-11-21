# Feature Route Modules

`api/src/routes/features/` contains feature-based route entrypoints. Legacy route files remain for compatibility, but app/v1 mounting should prefer feature modules.

## Modules

- `events.module.ts` - domain/indexer event API (`/api/events`, `/api/v1/events` if mounted)
- `lending.module.ts` - lending, gas, debt token, cross-asset, flash loan
- `protocol.module.ts` - protocol routes

## Migration pattern

1. Create `features/<feature>.module.ts`.
2. Mount subroutes inside the module.
3. Point `app.ts` and `routes/v1/*` to the feature module.
4. Keep legacy route files until consumers/tests migrate.
