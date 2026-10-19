# API Error Taxonomy

This document defines the machine-readable error codes returned by the StellarLend
API and the HTTP status each one maps to. The source of truth is the `ErrorCode`
enum in `api/src/utils/errors.ts`.

## Response envelope

Errors built from an `ApiError` subclass use the standard envelope, produced by
`api/src/middleware/errorHandler.ts`:

```json
{
  "success": false,
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid input",
    "details": {}
  },
  "requestId": "req_1729300000000_ab12cd34ef"
}
```

- `error.code` is one of the values in the table below. It is stable: the enum
  value is the wire string, so a code is never renamed, only deprecated.
- `error.details` is optional and only present when the error carries details.
- `requestId` echoes the `x-request-id` header, or is generated per request.
- Unknown (non-`ApiError`) exceptions are returned as `INTERNAL_SERVER_ERROR` with
  the generic message `Internal server error`. The underlying message is logged,
  not returned.

## Codes

| Code | HTTP status | Meaning |
| --- | --- | --- |
| `VALIDATION_ERROR` | 400 | The request is malformed or fails validation. Also used for invalid JSON bodies. |
| `UNAUTHORIZED` | 401 | Missing or invalid credentials. |
| `NOT_FOUND` | 404 | The requested resource does not exist. |
| `CONFLICT` | 409 | The request conflicts with current state, for example a duplicate. |
| `PAYLOAD_TOO_LARGE` | 413 | The request body exceeds the configured size limit. |
| `RATE_LIMITED` | 429 | Reserved. See *Known gaps*. |
| `INSUFFICIENT_BALANCE` | 4xx | Reserved. The account does not hold enough of the asset. Not yet emitted. |
| `CONTRACT_ERROR` | 422 | The simulated or submitted contract call failed, so fees cannot be estimated. |
| `SANCTIONED_ADDRESS` | 403 | The address is on the internal sanctions list. |
| `OFAC_MATCH` | 403 | The address matches the OFAC sanctions list. |
| `KYC_REQUIRED` | 403 | The operation requires completed KYC. |
| `RESTRICTED_JURISDICTION` | 403 | The caller's jurisdiction is restricted. |
| `LIMIT_EXCEEDED` | 403 | The operation exceeds a compliance limit. |
| `AML_HIGH_RISK` | 403 | AML screening rated the address high risk. |
| `AML_CRITICAL` | 403 | AML screening rated the address critical risk. |
| `NETWORK_ERROR` | 502 | An upstream dependency, such as the Soroban RPC, failed. |
| `INTERNAL_SERVER_ERROR` | 500 | An unexpected server-side failure. |

Compliance codes come from `api/src/middleware/complianceEnforcement.ts`. Those
responses currently return a top-level `error` string and a top-level `code`
rather than the nested envelope above. Clients should read `code` from either
location until the envelope is unified.

## Adding a code

1. Add the member to `ErrorCode` in `api/src/utils/errors.ts` with the same string
   as its name.
2. Document it in the table above with its status.
3. Add a case to `api/src/__tests__/errorTaxonomy.test.ts`.
4. Use the enum member at the call site. Do not inline string literals.

## Known gaps

- **Legacy string bodies.** Most routes still respond with
  `{ "error": "<message>" }` and no `code`. Migrating them to `ApiError` is
  tracked separately. New routes should throw an `ApiError` subclass.
- **Rate limiter.** `api/src/middleware/rate-limit.ts` returns a 429 with a string
  `error` and no `code`, so `RATE_LIMITED` is not yet emitted. Switching it to the
  envelope will be a visible change for clients that parse the body.
