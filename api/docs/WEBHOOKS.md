# Webhooks — Liquidations (#1376)

StellarLend sends signed webhook notifications when positions enter
liquidation range.

## Subscribing

```bash
curl -X POST https://api.stellarlend.example/api/developer/webhooks \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com/hooks/stellarlend","events":["liquidation.alert"]}'
```

The response includes a `secret` **once** — store it for signature verification.

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/api/developer/webhooks` | Register a subscription (`url`, optional `events`) |
| `GET` | `/api/developer/webhooks` | List subscriptions (secrets never returned) |
| `DELETE` | `/api/developer/webhooks/:id` | Remove a subscription |
| `POST` | `/api/developer/webhooks/test` | Deliver a signed test event immediately |

## Events

| Event | Fired when |
|---|---|
| `liquidation.alert` | One or more monitored positions are at/below their danger health factor |
| `webhook.test` | Manual test delivery |

## Signature

Every request carries:

```
X-StellarLend-Event: liquidation.alert
X-StellarLend-Delivery: evt_<id>
X-StellarLend-Signature: t=<unix-seconds>,v1=<hex-hmac-sha256>
Content-Type: application/json
```

The HMAC-SHA256 is computed over `${t}.${rawBody}` using your subscription
secret. Verify in constant time and reject timestamps older than 5 minutes:

```ts
import { createHmac, timingSafeEqual } from 'crypto';

function verify(secret: string, rawBody: string, header: string): boolean {
  const [tPart, v1Part] = header.split(',');
  const t = Number(tPart.slice(2));
  if (Math.abs(Date.now() / 1000 - t) > 300) return false;
  const expected = createHmac('sha256', secret).update(`${t}.${rawBody}`).digest('hex');
  return timingSafeEqual(Buffer.from(expected), Buffer.from(v1Part.slice(3)));
}
```

## Retries

Failed deliveries (non-2xx or network error) are retried up to **3 attempts**
with capped exponential backoff plus jitter:

`delay(n) = min(WEBHOOK_MAX_DELAY_MS, WEBHOOK_RETRY_BASE_MS * 2^(n-1)) + jitter`

Defaults: `WEBHOOK_RETRY_BASE_MS=100`, `WEBHOOK_MAX_DELAY_MS=2000`.

Deliveries are best-effort and in-memory in this slice; durable queuing is
tracked separately.
