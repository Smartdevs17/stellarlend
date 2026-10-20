# StellarLend Liquidation Bot

Automated off-chain bot that monitors position health on the StellarLend
protocol and executes liquidations when profitable.

Resolves **Issue #1402** — Add liquidation bot interface.

---

## Overview

The liquidation bot connects to the StellarLend API over both HTTP polling
and a WebSocket health-feed, identifies under-collateralised positions, and
submits liquidation transactions when the expected net profit exceeds a
configurable minimum threshold.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Liquidation Bot                               │
│                                                                  │
│  ┌──────────────┐   health_update    ┌───────────────────────┐  │
│  │  WebSocket   │ ─────────────────▶ │  evaluatePositions()  │  │
│  │  Feed        │                    │  • filter HF < 1.2    │  │
│  └──────────────┘                    │  • check min profit   │  │
│                                      └───────────┬───────────┘  │
│  ┌──────────────┐   HTTP poll                    │              │
│  │  REST API    │ ─────────────────────────────▶ │              │
│  │  /opport.    │                                ▼              │
│  └──────────────┘                   ┌────────────────────────┐  │
│                                     │  executeLiquidation()  │  │
│                                     │  POST /lending/liquidate│  │
│                                     └────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Quick Start

### Prerequisites

- Node.js ≥ 18
- A funded Stellar keypair on the target network
- Access to a running StellarLend API (local or testnet)

### Install & run

```bash
cd bots/liquidation-bot
cp .env.example .env
# fill in BOT_SECRET_KEY, BOT_PUBLIC_KEY, API_BASE_URL, WS_URL
npm install
npm start
```

### Dry-run mode (safe to test)

```bash
DRY_RUN=true npm start
```

In dry-run mode the bot evaluates positions and logs what it *would* do, but
makes no on-chain calls.

---

## Configuration

Copy `.env.example` to `.env` and edit:

| Variable | Default | Description |
|---|---|---|
| `STELLAR_NETWORK` | `testnet` | `testnet` or `mainnet` |
| `STELLAR_RPC_URL` | horizon-testnet | Soroban RPC endpoint |
| `STELLAR_HORIZON_URL` | horizon-testnet | Horizon endpoint |
| `BOT_SECRET_KEY` | *(required)* | Stellar secret key (`S...`) |
| `BOT_PUBLIC_KEY` | *(required)* | Corresponding public key (`G...`) |
| `MIN_PROFIT_THRESHOLD_XLM` | `1.0` | Minimum net profit to attempt liquidation |
| `MAX_GAS_PRICE_STROOPS` | `100000` | Max gas price per transaction |
| `LIQUIDATION_TARGET_STRATEGY` | `highest_profit` | `highest_profit` \| `highest_risk` \| `lowest_hf` |
| `POLL_INTERVAL_MS` | `5000` | How often to poll the REST API (ms) |
| `DRY_RUN` | `true` | Set to `false` to submit real transactions |
| `MAX_CONCURRENT_LIQUIDATIONS` | `3` | Max parallel liquidations in-flight |
| `WS_URL` | `ws://localhost:3000/…` | WebSocket health-feed URL |
| `API_BASE_URL` | `http://localhost:3000/api` | REST API base URL |
| `LIQUIDATION_WEBHOOK_URL` | *(disabled)* | URL for signed `liquidation.confirmed` notifications |
| `LIQUIDATION_WEBHOOK_SECRET` | *(disabled)* | Shared secret used for HMAC-SHA256 signatures |
| `WEBHOOK_MAX_ATTEMPTS` | `3` | Maximum delivery attempts, including the first request |
| `WEBHOOK_RETRY_DELAY_MS` | `500` | Initial delay between retry attempts; it doubles after each failure |
| `LOG_LEVEL` | `info` | `debug` \| `info` \| `warn` \| `error` |

---

## Position Discovery API

The bot discovers liquidatable positions by polling:

```
GET /api/liquidations/opportunities?maxHf=1.2&sortBy=netProfitStroops&sortDir=desc
```

Response shape:

```json
{
  "opportunities": [
    {
      "address": "GXXX...",
      "collateralAsset": "USDC",
      "debtAsset": "XLM",
      "collateralValue": 1500000,
      "debtValue": 1000000,
      "healthFactor": 0.98,
      "estimatedProfit": 50000,
      "gasCostStroops": 10000,
      "netProfit": 40000
    }
  ]
}
```

It also subscribes to real-time updates via WebSocket:

```
ws://<host>/api/ws/health-updates
```

Messages with `type: "health_update"` or `type: "health_snapshot"` carry
the same `PositionHealth` shape and are processed immediately.

---

## Liquidation Execution

For each selected position the bot calls:

```
POST /api/lending/liquidate
{
  "positionAddress": "GXXX...",
  "maxRepayAmount": "500000"
}
```

`maxRepayAmount` is set to 50% of the current debt value, staying within
the protocol's close-factor cap.

---

## Target Strategies

| Strategy | Behaviour |
|---|---|
| `highest_profit` | Sort by `netProfit` descending — maximise per-tx earnings |
| `highest_risk` | Sort by `healthFactor` ascending — prioritise most dangerous positions |
| `lowest_hf` | Alias for `highest_risk` |

---

## Health Check

The bot exposes runtime stats via `getStats()`:

```ts
const stats = bot.getStats();
// { totalAttempted: 12, totalSuccessful: 10, totalProfit: 1_200_000 }
```

Integrate with a process manager (e.g. PM2) to monitor the process and
restart on crash.

## Liquidation webhooks

When a real liquidation request succeeds, the bot sends a
`liquidation.confirmed` notification if both `LIQUIDATION_WEBHOOK_URL` and
`LIQUIDATION_WEBHOOK_SECRET` are configured. Dry-run results are not sent.

The JSON body has this shape:

```json
{
  "id": "liquidation_abc123_1710000000000",
  "type": "liquidation.confirmed",
  "createdAt": 1710000000000,
  "data": {
    "positionAddress": "GXXX...",
    "txHash": "abc123",
    "profitStroops": 40000,
    "timestamp": 1710000000000
  }
}
```

The request includes `x-stellarlend-webhook-id`,
`x-stellarlend-webhook-timestamp`, and
`x-stellarlend-webhook-signature` headers. The signature is
`sha256=` followed by the hexadecimal HMAC-SHA256 of
`<timestamp>.<raw request body>` using `LIQUIDATION_WEBHOOK_SECRET` as the
key. Consumers should compare the signature using a constant-time comparison
and reject timestamps outside their replay window before processing the event.

Network errors, timeouts, HTTP 429 responses, and HTTP 5xx responses are
retried up to `WEBHOOK_MAX_ATTEMPTS` with exponential backoff. Other HTTP
errors are not retried. Delivery is best effort and does not change the
already-recorded liquidation result; consumers should use the event `id` for
deduplication.

---

## Security Notes

- **Never commit your secret key.** Use environment variables or a secrets manager.
- Run in `DRY_RUN=true` on testnet first to validate your configuration.
- The bot connects only to the endpoints you configure — no external calls are
  made without your explicit configuration.
- Use `MAX_CONCURRENT_LIQUIDATIONS=1` in production until you have verified
  the bot's behaviour.

---

## Development

```bash
# Run tests
npm test

# Lint
npm run lint

# Build
npm run build
```

---

## Architecture

```
bots/liquidation-bot/
├── src/
│   ├── index.ts                  # Entry point — start/stop lifecycle
│   ├── liquidationBot.service.ts # Core bot logic
│   ├── auctionBidder.service.ts  # Dutch-auction bidding service
│   ├── config.ts                 # Environment-based configuration
│   └── logger.ts                 # Structured logger
├── .env.example                  # Template env file
├── package.json
├── tsconfig.json
└── README.md                     # This file
```

---

## Related Issues

- **#1402** — Add liquidation bot interface (this file)
- **#1403** — Add partial liquidation (`partial_liquidate` entrypoint in the contract)
- **#1400** — Flash loan support (callback-based, configurable fee)
- **#1401** — Flash loan reentrancy protection (RAII `FlashLoanGuard`)
