# API Sandbox Environment (#1377)

The sandbox provides an isolated, resettable environment for integrating
against StellarLend without touching chain state or production data.

## Enabling

Sandbox mode is enabled automatically outside production. In production it
requires an explicit opt-in:

```bash
SANDBOX_ENABLED=true
```

## Endpoints

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/sandbox` | Status: enabled flag, isolation model, resource counts, reset count |
| `POST` | `/api/sandbox/reset` | Wipe sandbox state and seed a demo key |

### `GET /api/sandbox`

```json
{
  "success": true,
  "data": {
    "enabled": true,
    "env": "development",
    "isolation": "in-memory",
    "resources": { "apiKeys": 1, "webhookSubscriptions": 0 },
    "resets": 0
  }
}
```

### `POST /api/sandbox/reset`

Clears all sandbox API keys and webhook subscriptions, then creates
`sandbox-demo-key` with the `read` scope. The raw key is returned **once**:

```json
{
  "success": true,
  "data": {
    "resetAt": "2026-10-19T00:00:00.000Z",
    "seededApiKey": {
      "rawKey": "<64-hex>",
      "record": { "id": "...", "name": "sandbox-demo-key", "scopes": ["read"] }
    }
  }
}
```

When sandbox mode is disabled the reset endpoint returns `403`.

## Isolation

Sandbox state lives in the process-local in-memory stores used by the
developer API key and webhook services. It never touches chain state, the
database, or production keys. Restarting the process also resets the
sandbox.
