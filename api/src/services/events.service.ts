/**
 * Structured event schema catalog (Issue #824).
 *
 * A machine-readable mirror of the on-chain event schema defined in
 * `stellar-lend/contracts/hello-world/src/events.rs`. Off-chain indexers,
 * dashboards, and webhook consumers use this as the single source of truth for:
 *   - the current schema version they should decode against,
 *   - the canonical module / action vocabulary,
 *   - the topic layout and field types of every emitted event.
 *
 * Keep `EVENT_SCHEMA_VERSION` and the `structured_event_v1` definition below in
 * lock-step with `EVENT_SCHEMA_VERSION` / `StructuredEventV1` in the contract.
 */

/**
 * Current version of the structured protocol event schema. Mirrors
 * `events::EVENT_SCHEMA_VERSION` in the `hello-world` contract.
 */
export const EVENT_SCHEMA_VERSION = 1 as const;

/** Logical subsystem that produced an event. Mirrors `events::EventModule`. */
export const EVENT_MODULES = [
  'lending',
  'collateral',
  'liquidation',
  'oracle',
  'governance',
  'treasury',
  'risk',
  'flash_loan',
  'admin',
  'emergency',
] as const;
export type EventModule = (typeof EVENT_MODULES)[number];

/** Canonical action verb. Mirrors `events::EventAction` (snake_case form). */
export const EVENT_ACTIONS = [
  'deposit',
  'withdraw',
  'borrow',
  'repay',
  'liquidate',
  'price_update',
  'params_update',
  'pause',
  'unpause',
  'proposal_created',
  'vote_cast',
  'execute',
  'claim',
  'flash_loan',
  'other',
] as const;
export type EventAction = (typeof EVENT_ACTIONS)[number];

export type FieldType =
  | 'address'
  | 'option<address>'
  | 'i128'
  | 'u32'
  | 'u64'
  | 'bool'
  | 'symbol'
  | 'string'
  | 'enum<EventModule>'
  | 'enum<EventAction>'
  | 'vec<StructuredEventField>';

export interface EventFieldDef {
  /** Field name as encoded in the Soroban event payload. */
  name: string;
  type: FieldType;
  /** Whether the field is published as an indexed topic (vs. data payload). */
  topic: boolean;
  description: string;
}

export interface EventDef {
  /** Canonical event name (matches the first non-prefix topic in snake_case). */
  name: string;
  module: EventModule;
  /** Canonical action, or `null` for envelope/multi-action events. */
  action: EventAction | null;
  /** Static first topic emitted for this event. */
  topicPrefix: string;
  /** Schema version this definition was introduced/last changed in. */
  schemaVersion: number;
  description: string;
  fields: EventFieldDef[];
}

export interface EventCatalog {
  schemaVersion: number;
  modules: readonly EventModule[];
  actions: readonly EventAction[];
  /** The versioned envelope emitted alongside every typed event. */
  envelope: string;
  events: EventDef[];
}

const TIMESTAMP_FIELD: EventFieldDef = {
  name: 'timestamp',
  type: 'u64',
  topic: false,
  description: 'Ledger timestamp captured at emission.',
};

const CORE_AMOUNT_EVENT_FIELDS = (verb: string): EventFieldDef[] => [
  {
    name: 'user',
    type: 'address',
    topic: true,
    description: `Account that performed the ${verb}.`,
  },
  {
    name: 'asset',
    type: 'option<address>',
    topic: false,
    description: 'Asset contract address; `null` for native XLM.',
  },
  {
    name: 'amount',
    type: 'i128',
    topic: false,
    description: `${verb} amount in base units (stroops).`,
  },
  TIMESTAMP_FIELD,
];

/**
 * The structured envelope. Indexers can subscribe to the
 * `("proto_evt", module, action, actor)` topic tuple and gate decoding on
 * `schema_version`.
 */
const STRUCTURED_EVENT_V1: EventDef = {
  name: 'structured_event_v1',
  module: 'lending',
  action: null,
  topicPrefix: 'proto_evt',
  schemaVersion: 1,
  description:
    'Versioned, self-describing envelope emitted alongside every typed event. ' +
    'Additive: it never replaces or alters a typed emission.',
  fields: [
    {
      name: 'module',
      type: 'enum<EventModule>',
      topic: true,
      description: 'Subsystem that produced the event.',
    },
    {
      name: 'action',
      type: 'enum<EventAction>',
      topic: true,
      description: 'Canonical action verb; `other` defers to `action_name`.',
    },
    {
      name: 'actor',
      type: 'address',
      topic: true,
      description: 'Primary account responsible for the state change.',
    },
    {
      name: 'schema_version',
      type: 'u32',
      topic: false,
      description: 'Schema version this payload conforms to.',
    },
    {
      name: 'action_name',
      type: 'symbol',
      topic: false,
      description: 'Concrete verb; equals `action` unless `action` is `other`.',
    },
    {
      name: 'asset',
      type: 'option<address>',
      topic: false,
      description: 'Primary asset the event concerns; `null` for native XLM / n-a.',
    },
    {
      name: 'amount',
      type: 'i128',
      topic: false,
      description: 'Primary signed amount in base units (0 when not applicable).',
    },
    {
      name: 'counterparty',
      type: 'option<address>',
      topic: false,
      description: 'Secondary party (borrower, delegatee, recipient…), if any.',
    },
    {
      name: 'metadata',
      type: 'vec<StructuredEventField>',
      topic: false,
      description: 'Structured numeric annotations: `{ key: symbol, value: i128 }[]`.',
    },
    TIMESTAMP_FIELD,
  ],
};

const TYPED_EVENTS: EventDef[] = [
  {
    name: 'deposit',
    module: 'lending',
    action: 'deposit',
    topicPrefix: 'deposit',
    schemaVersion: 1,
    description: 'Collateral or pool deposit.',
    fields: CORE_AMOUNT_EVENT_FIELDS('deposit'),
  },
  {
    name: 'withdrawal',
    module: 'lending',
    action: 'withdraw',
    topicPrefix: 'withdrawal',
    schemaVersion: 1,
    description: 'Collateral or pool withdrawal.',
    fields: CORE_AMOUNT_EVENT_FIELDS('withdrawal'),
  },
  {
    name: 'borrow',
    module: 'lending',
    action: 'borrow',
    topicPrefix: 'borrow',
    schemaVersion: 1,
    description: 'New debt drawn against collateral.',
    fields: CORE_AMOUNT_EVENT_FIELDS('borrow'),
  },
  {
    name: 'repay',
    module: 'lending',
    action: 'repay',
    topicPrefix: 'repay',
    schemaVersion: 1,
    description: 'Debt repayment.',
    fields: CORE_AMOUNT_EVENT_FIELDS('repayment'),
  },
  {
    name: 'liquidation',
    module: 'liquidation',
    action: 'liquidate',
    topicPrefix: 'liquidation',
    schemaVersion: 1,
    description: 'Undercollateralized position liquidated.',
    fields: [
      { name: 'liquidator', type: 'address', topic: true, description: 'Account performing the liquidation.' },
      { name: 'borrower', type: 'address', topic: true, description: 'Account being liquidated.' },
      { name: 'debt_asset', type: 'option<address>', topic: false, description: 'Repaid debt asset; `null` for native XLM.' },
      { name: 'collateral_asset', type: 'option<address>', topic: false, description: 'Seized collateral asset; `null` for native XLM.' },
      { name: 'debt_liquidated', type: 'i128', topic: false, description: 'Debt repaid by the liquidator.' },
      { name: 'collateral_seized', type: 'i128', topic: false, description: 'Collateral transferred to the liquidator.' },
      { name: 'incentive_amount', type: 'i128', topic: false, description: 'Liquidation bonus paid to the liquidator.' },
      TIMESTAMP_FIELD,
    ],
  },
  {
    name: 'price_updated',
    module: 'oracle',
    action: 'price_update',
    topicPrefix: 'price_updated',
    schemaVersion: 1,
    description: 'Oracle price for an asset was updated.',
    fields: [
      { name: 'actor', type: 'address', topic: false, description: 'Account that submitted the update.' },
      { name: 'asset', type: 'address', topic: true, description: 'Asset the price refers to.' },
      { name: 'price', type: 'i128', topic: false, description: 'New price, scaled by `decimals`.' },
      { name: 'decimals', type: 'u32', topic: false, description: 'Fixed-point decimals for `price`.' },
      { name: 'oracle', type: 'address', topic: false, description: 'Oracle source contract.' },
      TIMESTAMP_FIELD,
    ],
  },
  {
    name: 'pause_state_changed',
    module: 'admin',
    action: 'pause',
    topicPrefix: 'pause_state_changed',
    schemaVersion: 1,
    description: 'An operation was paused or unpaused.',
    fields: [
      { name: 'actor', type: 'address', topic: false, description: 'Admin that changed the state.' },
      { name: 'operation', type: 'symbol', topic: false, description: 'Operation identifier that was (un)paused.' },
      { name: 'paused', type: 'bool', topic: false, description: '`true` = paused, `false` = resumed.' },
      TIMESTAMP_FIELD,
    ],
  },
  {
    name: 'flash_loan_initiated',
    module: 'flash_loan',
    action: 'flash_loan',
    topicPrefix: 'flash_loan_initiated',
    schemaVersion: 1,
    description: 'A flash loan was disbursed to a receiver.',
    fields: [
      { name: 'user', type: 'address', topic: true, description: 'Borrower / initiator.' },
      { name: 'asset', type: 'address', topic: true, description: 'Borrowed asset.' },
      { name: 'amount', type: 'i128', topic: false, description: 'Borrowed amount.' },
      { name: 'fee', type: 'i128', topic: false, description: 'Fee owed on repayment.' },
      { name: 'callback', type: 'address', topic: false, description: 'Contract invoked with the borrowed funds.' },
      TIMESTAMP_FIELD,
    ],
  },
  {
    name: 'flash_loan_repaid',
    module: 'flash_loan',
    action: 'flash_loan',
    topicPrefix: 'flash_loan_repaid',
    schemaVersion: 1,
    description: 'A flash loan principal + fee was repaid.',
    fields: [
      { name: 'user', type: 'address', topic: true, description: 'Borrower / initiator.' },
      { name: 'asset', type: 'address', topic: true, description: 'Borrowed asset.' },
      { name: 'amount', type: 'i128', topic: false, description: 'Principal repaid.' },
      { name: 'fee', type: 'i128', topic: false, description: 'Fee repaid.' },
      TIMESTAMP_FIELD,
    ],
  },
  {
    name: 'admin_action',
    module: 'admin',
    action: 'other',
    topicPrefix: 'admin_action',
    schemaVersion: 1,
    description: 'Generic privileged administrative action.',
    fields: [
      { name: 'actor', type: 'address', topic: false, description: 'Admin that performed the action.' },
      { name: 'action', type: 'symbol', topic: false, description: 'Action identifier.' },
      TIMESTAMP_FIELD,
    ],
  },
];

const ALL_EVENTS: EventDef[] = [STRUCTURED_EVENT_V1, ...TYPED_EVENTS];

const EVENTS_BY_NAME: ReadonlyMap<string, EventDef> = new Map(
  ALL_EVENTS.map((e) => [e.name, e])
);

/** Full catalog: schema version, vocabularies, and every event definition. */
export function getEventCatalog(): EventCatalog {
  return {
    schemaVersion: EVENT_SCHEMA_VERSION,
    modules: EVENT_MODULES,
    actions: EVENT_ACTIONS,
    envelope: STRUCTURED_EVENT_V1.name,
    events: ALL_EVENTS,
  };
}

/** Look up a single event definition by its canonical name. */
export function getEventDefinition(name: string): EventDef | undefined {
  return EVENTS_BY_NAME.get(name);
}

/** All known module identifiers. */
export function listModules(): readonly EventModule[] {
  return EVENT_MODULES;
}

/** All known canonical action verbs. */
export function listActions(): readonly EventAction[] {
  return EVENT_ACTIONS;
}
