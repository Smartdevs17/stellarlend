# Domain Event Bus

`api/src/events/` provides a small domain-event bus between contract/indexer ingestion and off-chain consumers.

## Flow

1. Soroban events are fetched and normalized by `services/eventIndex`.
2. `EventIndexer.ingest()` stores newly indexed events.
3. Newly stored events are published to the domain bus.
4. Off-chain consumers subscribe by domain event name or wildcard.

## Usage

```ts
import { getDomainEventBus, DOMAIN_EVENT_NAMES } from '../events';

const bus = getDomainEventBus();
const sub = bus.subscribe(DOMAIN_EVENT_NAMES.LendingDeposit, async (event) => {
  // handle event.payload
});

// later
sub.unsubscribe();
```

## Guarantees

- **At-most-once per ingest**: only events newly added to the store are published.
- **Isolated handlers**: handler failures are logged and counted; they do not fail ingestion.
- **Traceable**: domain events include `source` indexed event, ledger, contract, and id.
- **Extensible**: map new contract/indexer types in `mapper.ts`; add consumers via `subscribe`.
