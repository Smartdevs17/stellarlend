# Transaction Simulation and Fee Estimation

Two endpoints let a client see what a transaction will do and what it will cost before signing it. Both take the same input: either a base64 transaction envelope (`transactionXdr`) or a lending operation specification that the API builds exactly as `GET /api/lending/prepare/:operation` does.

| Endpoint | Purpose |
|----------|---------|
| `POST /api/lending/simulate` (also `/api/v1/lending/simulate`) | Full simulation result: resources, minimum resource fee, decoded return value, required authorizations, diagnostic events, state changes, restore preamble |
| `POST /api/gas/estimate-transaction` (also `/api/v1/lending/gas/estimate-transaction`) | Fee breakdown for the transaction with a configurable safety margin |

Both run one Soroban RPC `simulateTransaction` call. Results are cached for `SIMULATION_CACHE_TTL_MS` (default 10 s) keyed by the SHA-256 of the envelope, so a wallet that polls the same unsigned transaction reuses the RPC call. Every submitted transaction drops the simulation cache.

## Request body

```json
{ "transactionXdr": "AAAAAgAAAAB..." }
```

or

```json
{
  "operation": "borrow",
  "userAddress": "G...",
  "amount": "5000000",
  "assetAddress": "C..."
}
```

| Field | Rules |
|-------|-------|
| `transactionXdr` | Base64 envelope, at most 20000 characters. When present the operation fields are ignored. Fee-bump envelopes are accepted; the inner transaction is inspected. |
| `operation` | One of `deposit`, `borrow`, `repay`, `withdraw` |
| `userAddress` | Ed25519 public key; the account sequence is fetched from Horizon |
| `amount` | Positive integer string in stroops |
| `assetAddress` | Optional contract address; omitted means the native pool |
| `feeMarginPercent` | Estimate endpoint only. Integer 0 to 100, default 10 |

Validation failures return `400` with the standard error envelope and a message naming the field.

## `POST /api/lending/simulate`

```json
{
  "success": true,
  "status": "success",
  "transactionXdr": "AAAAAgAAAAB...",
  "sourceAccount": "G...",
  "operationCount": 1,
  "latestLedger": 4242,
  "minResourceFee": "40100",
  "resources": {
    "cpuInstructions": "250000",
    "readBytes": "1024",
    "writeBytes": "256",
    "readOnlyEntries": 3,
    "readWriteEntries": 1,
    "resourceFee": "40000"
  },
  "memoryBytes": null,
  "result": {
    "retvalXdr": "AAAACgAAAAA...",
    "retval": "5000000",
    "auth": [
      { "xdr": "AAAAAAAAAAE...", "credentials": "source_account" },
      { "xdr": "AAAAAQAAAAA...", "credentials": "address", "address": "G..." }
    ]
  },
  "events": ["AAAAAQAAAAA..."],
  "stateChanges": [{ "type": "updated", "keyXdr": "AAAABgAAAAE..." }],
  "restorePreamble": null,
  "error": null,
  "cached": false,
  "simulatedAt": "2026-09-29T12:00:00.000Z"
}
```

`status` is one of:

- `success`: the invocation ran; `result` holds the decoded return value (bigints as strings, bytes as base64) and the authorization entries the signer must provide.
- `restore_required`: the invocation ran as if archived ledger entries were present. `restorePreamble.minResourceFee` and `restorePreamble.resources` describe the restore transaction that must be submitted first.
- `error`: the host rejected the invocation. The response is still `200`; `error` carries the host error and `events` the diagnostic events, which is what a client needs to explain the failure.

`memoryBytes` is only reported when the RPC node returns a cost block; the SDK's parsed response does not carry it.

`502` means Soroban RPC could not be reached.

## `POST /api/gas/estimate-transaction`

```json
{
  "success": true,
  "status": "success",
  "sourceAccount": "G...",
  "operationCount": 1,
  "latestLedger": 4242,
  "fees": {
    "baseFee": "100",
    "operationCount": 1,
    "inclusionFee": "100",
    "resourceFee": "40100",
    "totalFee": "40200",
    "feeMarginPercent": 10,
    "recommendedFee": "44220"
  },
  "resources": { "cpuInstructions": "250000", "readBytes": "1024", "writeBytes": "256", "readOnlyEntries": 3, "readWriteEntries": 1, "resourceFee": "40000" },
  "memoryBytes": null,
  "restoreRequired": false,
  "restorePreamble": null,
  "cached": false,
  "simulatedAt": "2026-09-29T12:00:00.000Z"
}
```

Fee arithmetic, all in stroops with BigInt:

- `inclusionFee = baseFee * operationCount` (base fee 100 stroops)
- `totalFee = inclusionFee + minResourceFee`
- `recommendedFee = totalFee + ceil(totalFee * feeMarginPercent / 100)`

Set `recommendedFee` as the transaction fee when signing. When `restoreRequired` is true, the restore transaction pays its own `restorePreamble.minResourceFee` on top.

A failed simulation cannot produce a fee and returns `422` with code `CONTRACT_ERROR`; `details.simulationError` and `details.events` explain why.

## Examples

```bash
curl -X POST http://localhost:3000/api/lending/simulate \
  -H 'Content-Type: application/json' \
  -d '{"operation":"deposit","userAddress":"G...","amount":"10000000"}'

curl -X POST http://localhost:3000/api/gas/estimate-transaction \
  -H 'Content-Type: application/json' \
  -d '{"transactionXdr":"AAAAAgAAAAB...","feeMarginPercent":15}'
```

## Implementation notes

- `api/src/services/transactionSimulation.service.ts`: request parsing, envelope parsing, operation building, RPC call and `normalizeSimulation`, a pure function that accepts both the SDK's parsed response and the raw JSON-RPC shape.
- `api/src/services/sorobanFees.ts`: `summarizeSorobanData` (reads `SorobanTransactionData` resources and footprint) and `estimateFees`.
- `api/src/controllers/transactionSimulation.controller.ts`: the two handlers.
- Tests: `transactionSimulation.service.test.ts`, `transactionSimulation.routes.test.ts`, `sorobanFees.test.ts`.
