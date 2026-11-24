# Pool whitelisting gated by risk-council quorum

A pool can only be whitelisted after a quorum of risk-council members approves it. The API implements this under `/api/pools` (`api/src/routes/pools.ts`, issue #1528).

## Configuration

The council is read from the environment on every request.

| Variable | Meaning |
| --- | --- |
| `RISK_COUNCIL_MEMBERS` | Comma-separated Stellar addresses of council members |
| `RISK_COUNCIL_QUORUM` | Optional. Approvals needed. Defaults to a strict majority: `floor(members / 2) + 1` |

The whitelist fails closed:

- With no members configured, nobody can propose or approve, and requests return `403`.
- If the quorum is larger than the member count, requests return `500` and nothing is whitelisted. A misconfigured council never whitelists a pool by accident.

## Rules

- Only a council member can open a proposal, and only a council member can approve one.
- The proposer's approval counts toward quorum.
- Each member approves a given proposal at most once.
- Reaching quorum whitelists the pool immediately, in the same request.
- A pool with a pending proposal cannot receive a second one, and a whitelisted pool cannot be proposed again.
- The pool must be a Stellar contract address: `C` followed by 55 base32 characters.

## Endpoints

| Method | Path | Auth | Purpose |
| --- | --- | --- | --- |
| `GET` | `/api/pools/whitelist` | Public | Whitelisted pools |
| `GET` | `/api/pools/whitelist/proposals` | Public | All proposals, pending and executed |
| `GET` | `/api/pools/whitelist/proposals/:id` | Public | One proposal |
| `POST` | `/api/pools/whitelist/proposals` | Council member | Body `{ "pool": "C…" }`; opens a proposal (201) |
| `POST` | `/api/pools/whitelist/proposals/:id/approvals` | Council member | Records an approval (200) |
| `GET` | `/api/pools/whitelist/:pool` | Public | Whitelist status for one pool |

Requests use the standard `Authorization: Bearer <token>` header. Errors use the API's standard envelope. Status codes: `400` invalid pool, `401` no token, `403` not a council member, `404` unknown proposal, `409` duplicate approval, duplicate proposal, or already executed, `500` misconfigured quorum.

## Current limitations

- State lives in memory, like the other governance services in this API. It resets on restart. Durable storage belongs with the on-chain wiring.
- Whitelisting is recorded in the API only. Nothing yet writes the decision to a contract. Treat the API status as advisory until that wiring lands.
