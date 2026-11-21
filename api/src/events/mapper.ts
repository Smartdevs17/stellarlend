import type { IndexedEvent } from '../services/eventIndex/types';
import { DOMAIN_EVENT_NAMES } from './names';
import { DOMAIN_EVENT_VERSION, type DomainEvent } from './types';

function base(event: IndexedEvent, name: string, payload: Record<string, unknown>): DomainEvent {
  return {
    id: event.id,
    name,
    version: DOMAIN_EVENT_VERSION,
    occurredAt: event.timestamp,
    ledger: event.ledger,
    contract: event.contract,
    sourceType: event.type,
    source: event,
    payload,
  };
}

/**
 * Map an indexed contract event to a domain event.
 *
 * Returns null for events that are intentionally not projected into the
 * domain bus (unknown/noisy events can still be observed via subscribeAll).
 */
export function mapIndexedEventToDomainEvent(event: IndexedEvent): DomainEvent | null {
  const common = {
    eventId: event.id,
    ledger: event.ledger,
    contract: event.contract,
    actor: event.actor ?? null,
    counterparty: event.counterparty ?? null,
    asset: event.asset ?? (typeof event.data.asset === 'string' ? event.data.asset : null),
    amount:
      event.amount ?? (typeof event.data.amount === 'string' ? event.data.amount : null),
    timestamp: event.timestamp,
  };

  // Structured envelope carries module/action; prefer it when present.
  if (event.envelope === 'structured_event_v1') {
    switch (event.action) {
      case 'deposit':
        return base(event, DOMAIN_EVENT_NAMES.LendingDeposit, { ...common, module: event.module, action: event.action });
      case 'withdraw':
        return base(event, DOMAIN_EVENT_NAMES.LendingWithdraw, { ...common, module: event.module, action: event.action });
      case 'borrow':
        return base(event, DOMAIN_EVENT_NAMES.LendingBorrow, { ...common, module: event.module, action: event.action });
      case 'repay':
        return base(event, DOMAIN_EVENT_NAMES.LendingRepay, { ...common, module: event.module, action: event.action });
      case 'liquidate':
        return base(event, DOMAIN_EVENT_NAMES.Liquidation, { ...common, module: event.module, action: event.action });
      case 'flash_loan':
        return base(event, DOMAIN_EVENT_NAMES.FlashLoan, { ...common, module: event.module, action: event.action });
      case 'price_update':
        return base(event, DOMAIN_EVENT_NAMES.OraclePriceUpdated, { ...common, module: event.module, action: event.action, data: event.data });
      case 'params_update':
        return base(event, DOMAIN_EVENT_NAMES.RiskParamsUpdated, { ...common, module: event.module, action: event.action, data: event.data });
      case 'pause':
      case 'unpause':
        return base(event, DOMAIN_EVENT_NAMES.ProtocolPaused, { ...common, module: event.module, action: event.action, data: event.data });
      case 'proposal_created':
      case 'vote_cast':
      case 'execute':
        return base(event, DOMAIN_EVENT_NAMES.Governance, { ...common, module: event.module, action: event.action, data: event.data });
      default:
        return base(event, DOMAIN_EVENT_NAMES.GenericContractEvent, { ...common, module: event.module ?? null, action: event.action ?? null, data: event.data });
    }
  }

  switch (event.type) {
    case 'deposit':
    case 'vault_deposit':
    case 'borrow_collateral_deposit':
      return base(event, DOMAIN_EVENT_NAMES.LendingDeposit, common);
    case 'withdrawal':
    case 'withdraw':
      return base(event, DOMAIN_EVENT_NAMES.LendingWithdraw, common);
    case 'borrow':
      return base(event, DOMAIN_EVENT_NAMES.LendingBorrow, common);
    case 'repay':
      return base(event, DOMAIN_EVENT_NAMES.LendingRepay, common);
    case 'liquidation':
      return base(event, DOMAIN_EVENT_NAMES.Liquidation, {
        ...common,
        debtAsset: event.data.debt_asset ?? null,
        debtAmount: event.data.debt_liquidated ?? event.data.debt_amount ?? null,
        collateralAsset: event.data.collateral_asset ?? null,
        collateralAmount: event.data.collateral_seized ?? null,
      });
    case 'flash_loan_initiated':
    case 'flash_loan_repaid':
      return base(event, DOMAIN_EVENT_NAMES.FlashLoan, { ...common, phase: event.type });
    case 'price_updated':
      return base(event, DOMAIN_EVENT_NAMES.OraclePriceUpdated, { ...common, data: event.data });
    case 'risk_params_updated':
      return base(event, DOMAIN_EVENT_NAMES.RiskParamsUpdated, { ...common, data: event.data });
    case 'pause_state_changed':
      return base(event, DOMAIN_EVENT_NAMES.ProtocolPaused, { ...common, data: event.data });
    default:
      return base(event, DOMAIN_EVENT_NAMES.GenericContractEvent, { ...common, data: event.data });
  }
}
