/** Domain event names emitted by the bus. Keep stable; add, don't rename. */
export const DOMAIN_EVENT_NAMES = {
  LendingDeposit: 'lending.deposit',
  LendingWithdraw: 'lending.withdraw',
  LendingBorrow: 'lending.borrow',
  LendingRepay: 'lending.repay',
  Liquidation: 'liquidation.liquidate',
  FlashLoan: 'lending.flash_loan',
  Governance: 'governance.action',
  OraclePriceUpdated: 'oracle.price_updated',
  RiskParamsUpdated: 'risk.params_updated',
  ProtocolPaused: 'protocol.pause_state_changed',
  GenericContractEvent: 'contract.event',
} as const;

export type DomainEventName = (typeof DOMAIN_EVENT_NAMES)[keyof typeof DOMAIN_EVENT_NAMES];
