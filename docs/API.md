# StellarLend API Documentation

## Overview

StellarLend provides a comprehensive REST API for interacting with the DeFi lending protocol. This documentation covers all available endpoints, authentication methods, request/response formats, and error handling.

## Base URL

```
https://api.stellarlend.io/api
```

## Authentication

### Bearer Token Authentication

Most API endpoints require authentication using JWT Bearer tokens.

```bash
curl -X GET https://api.stellarlend.io/api/positions \
  -H "Authorization: Bearer YOUR_JWT_TOKEN"
```

### API Key Authentication

Alternative authentication method using X-API-Key header:

```bash
curl -X GET https://api.stellarlend.io/api/positions \
  -H "X-API-Key: your-api-key"
```

### Stellar Signature Authentication

For sensitive operations, cryptographic signature verification is required:

```bash
curl -X POST https://api.stellarlend.io/api/credit/create \
  -H "X-User-Address: GXXXX..." \
  -H "X-Stellar-Signature: base64-encoded-signature" \
  -H "X-Payload-Timestamp: 1234567890" \
  -H "Content-Type: application/json" \
  -d '{...}'
```

## Core Endpoints

### Lending

#### Deposit Collateral

```
POST /v1/lending/deposit
```

Deposit collateral into the protocol.

**Request:**
```json
{
  "asset": "USDC",
  "amount": "1000.00",
  "userAddress": "GXXXX..."
}
```

**Response:**
```json
{
  "success": true,
  "depositId": "uuid",
  "transaction": {
    "hash": "tx-hash",
    "status": "confirmed"
  }
}
```

#### Borrow Assets

```
POST /v1/lending/borrow
```

Borrow assets against deposited collateral.

**Request:**
```json
{
  "asset": "USDT",
  "amount": "500.00",
  "userAddress": "GXXXX..."
}
```

**Response:**
```json
{
  "success": true,
  "borrowId": "uuid",
  "interestRate": "0.05",
  "dueDate": "2025-12-24T00:00:00Z"
}
```

#### Get Positions

```
GET /v1/lending/positions/:userAddress
```

Retrieve all lending positions for a user.

**Response:**
```json
{
  "success": true,
  "positions": [
    {
      "positionId": "uuid",
      "collateral": [
        {
          "asset": "USDC",
          "amount": "1000.00"
        }
      ],
      "borrowed": [
        {
          "asset": "USDT",
          "amount": "500.00",
          "interestAccrued": "2.50"
        }
      ],
      "healthFactor": 2.5
    }
  ]
}
```

### Transactions

#### Create Multi-Step Transaction

```
POST /transactions
```

Requires authentication. Create a multi-step transaction workflow.

**Request:**
```json
{
  "userAddress": "GXXXX...",
  "description": "Swap and deposit",
  "steps": [
    {
      "operation": "swap",
      "amount": "100",
      "assetAddress": "CAAAA..."
    },
    {
      "operation": "deposit",
      "amount": "100",
      "assetAddress": "CBBBB..."
    }
  ],
  "ttlSeconds": 600
}
```

**Response:**
```json
{
  "success": true,
  "transaction": {
    "txId": "uuid",
    "userAddress": "GXXXX...",
    "status": "building",
    "steps": [...]
  }
}
```

#### List User Transactions

```
GET /transactions/user/:userAddress
```

Requires authentication. List all transactions for a user.

**Response:**
```json
{
  "success": true,
  "transactions": [...],
  "total": 10
}
```

#### Prepare Transaction Step

```
POST /transactions/:txId/steps/:stepId/prepare
```

Requires authentication and ownership validation.

#### Approve Transaction Step

```
POST /transactions/steps/approve
```

Requires authentication and ownership validation.

### Credit Lines

#### Create Credit Line

```
POST /credit/create
```

Requires Stellar signature authentication.

**Request:**
```json
{
  "delegateAddress": "GXXXX...",
  "maxAmount": "10000.00",
  "interestRate": "0.06",
  "maturityDate": "2025-12-31T23:59:59Z",
  "collateral": "USDC"
}
```

#### Draw from Credit Line

```
POST /credit/:id/draw
```

Requires Stellar signature authentication.

#### Repay Credit Line

```
POST /credit/:id/repay
```

Requires Stellar signature authentication.

### Compliance

#### Check Sanctions

```
GET /v1/compliance/sanctions/check?address=GXXXX...
```

Requires admin authentication.

#### Check KYC Status

```
GET /v1/compliance/kyc/check?address=GXXXX...
```

Requires admin authentication.

#### Add Sanctions Record

```
POST /v1/compliance/sanctions
```

Requires admin authentication.

**Request:**
```json
{
  "address": "GXXXX...",
  "jurisdiction": "US",
  "reason": "OFAC listing"
}
```

## Error Handling

All errors follow a standard format:

```json
{
  "success": false,
  "error": "Error message",
  "code": "ERROR_CODE",
  "details": {}
}
```

### Common Error Codes

- `UNAUTHORIZED` (401): Missing or invalid authentication
- `FORBIDDEN` (403): Insufficient permissions
- `NOT_FOUND` (404): Resource not found
- `VALIDATION_ERROR` (400): Invalid request parameters
- `RATE_LIMITED` (429): Too many requests
- `INTERNAL_ERROR` (500): Server error

## Rate Limiting

API requests are rate-limited to 1000 requests per minute per API key.

Headers included in each response:

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1234567890
```

## Webhooks

Subscribe to protocol events via webhooks:

```
POST /v1/webhooks/subscribe
```

**Request:**
```json
{
  "url": "https://your-domain.com/webhook",
  "events": [
    "position.created",
    "transaction.completed",
    "liquidation.triggered"
  ]
}
```

## Code Examples

See `/docs/examples/` for comprehensive code samples in multiple languages.
