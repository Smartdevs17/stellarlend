#![allow(unused_variables)]
//! Contract event definitions and emitter helpers for the `hello-world` lending
//! contract.
//!
//! # Event model
//!
//! Two complementary layers are emitted:
//!
//! 1. **Strongly-typed events** (`DepositEvent`, `LiquidationEvent`, …) – the
//!    source of truth for payload detail. Each is published via the Soroban
//!    `contractevent` macro and its generated `publish` helper.
//! 2. **Structured event schema** ([`StructuredEventV1`]) – a single versioned
//!    envelope emitted alongside the typed events. Off-chain indexers can
//!    subscribe to one topic prefix (`("proto_evt", module, action, actor)`)
//!    rather than tracking every bespoke event name, and can rely on a stable
//!    [`EVENT_SCHEMA_VERSION`] contract for forward compatibility.
//!
//! The structured layer is **additive**: it never replaces or alters an
//! existing typed emission, so consumers of the typed events are unaffected.
//! See `api/src/routes/events.ts` for the machine-readable catalog served to
//! off-chain consumers.

pub use shared_events::*;

use soroban_sdk::{contractevent, contracttype, Address, Env, String, Symbol, Vec};

use crate::types::{AssetStatus, ProposalType, VoteType};

// ============================================================================
// Core Lending Events (Existing)
// ============================================================================





























// ============================================================================
// Asset-Specific Events (Carbon Asset Style)
// ============================================================================

#[allow(dead_code)]


#[allow(dead_code)]


#[allow(dead_code)]


#[allow(dead_code)]


#[allow(dead_code)]


#[allow(dead_code)]


#[allow(dead_code)]


// ============================================================================
// Governance Events
// ============================================================================



























#[allow(dead_code)]


// ============================================================================
// Multisig Events
// ============================================================================

#[allow(dead_code)]


// ============================================================================
// Guardian & Recovery Events
// ============================================================================





#[allow(dead_code)]








// ============================================================================
// Treasury Events
// ============================================================================









// ============================================================================
// Core Lending Emitter Helpers
// ============================================================================

pub fn emit_deposit(e: &Env, event: DepositEvent) {
    event.publish(e);
}

pub fn emit_withdrawal(e: &Env, event: WithdrawalEvent) {
    event.publish(e);
}

pub fn emit_borrow(e: &Env, event: BorrowEvent) {
    event.publish(e);
}

pub fn emit_repay(e: &Env, event: RepayEvent) {
    event.publish(e);
}

pub fn emit_liquidation(e: &Env, event: LiquidationEvent) {
    event.publish(e);
}

pub fn emit_flash_loan_initiated(e: &Env, event: FlashLoanInitiatedEvent) {
    event.publish(e);
}

pub fn emit_flash_loan_repaid(e: &Env, event: FlashLoanRepaidEvent) {
    event.publish(e);
}

pub fn emit_admin_action(e: &Env, event: AdminActionEvent) {
    event.publish(e);
}

pub fn emit_price_updated(e: &Env, event: PriceUpdatedEvent) {
    event.publish(e);
}

pub fn emit_risk_params_updated(e: &Env, event: RiskParamsUpdatedEvent) {
    event.publish(e);
}

pub fn emit_pause_state_changed(e: &Env, event: PauseStateChangedEvent) {
    event.publish(e);
}

pub fn emit_position_updated(e: &Env, event: PositionUpdatedEvent) {
    event.publish(e);
}

pub fn emit_analytics_updated(e: &Env, event: AnalyticsUpdatedEvent) {
    event.publish(e);
}

pub fn emit_user_activity_tracked(e: &Env, event: UserActivityTrackedEvent) {
    event.publish(e);
}

// ============================================================================
// Asset-Specific Emitter Helpers
// ============================================================================

#[allow(dead_code)]
pub fn emit_mint(e: &Env, event: MintEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_transfer(e: &Env, event: TransferEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_status_change(e: &Env, event: StatusChangeEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_quality_score_updated(e: &Env, event: QualityScoreUpdatedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_approve(e: &Env, event: ApproveEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_sep41_transfer(e: &Env, event: Sep41TransferEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_sep41_burn(e: &Env, event: Sep41BurnEvent) {
    event.publish(e);
}

// ============================================================================
// Governance Emitter Helpers
// ============================================================================

#[allow(dead_code)]
pub fn emit_governance_initialized(e: &Env, event: GovernanceInitializedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_proposal_created(e: &Env, event: ProposalCreatedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_vote_cast(e: &Env, event: VoteCastEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_proposal_queued(e: &Env, event: ProposalQueuedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_proposal_executed(e: &Env, event: ProposalExecutedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_proposal_failed(e: &Env, event: ProposalFailedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_proposal_cancelled(e: &Env, event: ProposalCancelledEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_proposal_approved(e: &Env, event: ProposalApprovedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_governance_config_updated(e: &Env, event: GovernanceConfigUpdatedEvent) {
    event.publish(e);
}

// ============================================================================
// Multisig Emitter Helpers
// ============================================================================

#[allow(dead_code)]
pub fn emit_multisig_config_updated(e: &Env, event: MultisigConfigUpdatedEvent) {
    event.publish(e);
}

// ============================================================================
// Guardian & Recovery Emitter Helpers
// ============================================================================

#[allow(dead_code)]
pub fn emit_guardian_added(e: &Env, event: GuardianAddedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_guardian_removed(e: &Env, event: GuardianRemovedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_guardian_threshold_updated(e: &Env, event: GuardianThresholdUpdatedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_recovery_started(e: &Env, event: RecoveryStartedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_recovery_approved(e: &Env, event: RecoveryApprovedEvent) {
    event.publish(e);
}

#[allow(dead_code)]
pub fn emit_recovery_executed(e: &Env, event: RecoveryExecutedEvent) {
    event.publish(e);
}

// ============================================================================
// Treasury Emitter Helpers
// ============================================================================

pub fn emit_treasury_set(e: &Env, event: TreasurySetEvent) {
    event.publish(e);
}

pub fn emit_reserves_claimed(e: &Env, event: ReservesClaimedEvent) {
    event.publish(e);
}

pub fn emit_fee_config_updated(e: &Env, event: FeeConfigUpdatedEvent) {
    event.publish(e);
}

pub fn emit_liquidation_fee_collected(e: &Env, event: LiquidationFeeCollectedEvent) {
    event.publish(e);
}

/// Combo flash-loan + liquidation execution (Issue #661).
#[contractevent(topics = ["flash_liq_combo"])]
#[derive(Clone, Debug)]
pub struct FlashLoanLiquidationComboEvent {
    #[topic]
    pub liquidator: Address,
    #[topic]
    pub borrower: Address,
    pub debt_asset: Address,
    pub debt_amount: i128,
    pub collateral_seized: i128,
    pub flash_fee: i128,
    pub profit: i128,
    pub timestamp: u64,
}

pub fn emit_flash_loan_liquidation_combo(e: &Env, event: FlashLoanLiquidationComboEvent) {
    event.publish(e);
}

// ============================================================================
// Credit Scoring Events (from origin/main)
// ============================================================================



// ============================================================================
// Timelock Events (from origin/main)
// ============================================================================







// ============================================================================
// Circuit Breaker Events (from origin/main)
// ============================================================================









// ============================================================================
// Liquidation Queue Events (from HEAD)
// ============================================================================









// ============================================================================
// Emergency Withdrawal Events
// ============================================================================







pub fn emit_emergency_triggered(e: &Env, state: crate::types::EmergencyState) {
    EmergencyTriggeredEvent {
        trigger: state.trigger,
        started_at: state.started_at,
        window_opens_at: state.window_opens_at,
        window_closes_at: state.window_closes_at,
        timestamp: e.ledger().timestamp(),
    }
    .publish(e);
}

pub fn emit_emergency_cancelled(e: &Env) {
    EmergencyCancelledEvent {
        timestamp: e.ledger().timestamp(),
    }
    .publish(e);
}

pub fn emit_emergency_withdrawal(e: &Env, withdrawal: crate::types::EmergencyWithdrawal) {
    EmergencyWithdrawalEvent {
        user: withdrawal.user,
        asset: withdrawal.asset,
        amount: withdrawal.amount,
        loss_share_bps: withdrawal.loss_share_bps,
        timestamp: withdrawal.withdrawn_at,
    }
    .publish(e);
}

pub fn emit_batch_liquidation(e: &Env, event: BatchLiquidationEvent) {
    event.publish(e);
}

// ============================================================================
// Structured Event Schema (Issue #824)
// ============================================================================
//
// A single, versioned envelope emitted alongside the strongly-typed events
// above. Indexers subscribe to the `("proto_evt", module, action, actor)` topic
// tuple instead of tracking every bespoke event name, and gate their decoders
// on `schema_version`. This layer is additive — it does not replace or change
// any existing emission.

/// Current version of the structured protocol event schema.
///
/// Bump this on any backwards-incompatible change to [`StructuredEventV1`]
/// (field removal, reorder, or type change). Appending an optional
/// [`StructuredEventField`] to `metadata` does not require a bump.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

/// Logical subsystem that produced an event.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventModule {
    Lending,
    Collateral,
    Liquidation,
    Oracle,
    Governance,
    Treasury,
    Risk,
    FlashLoan,
    Admin,
    Emergency,
}

/// Canonical action verb shared across modules.
///
/// `Other` defers to [`StructuredEventV1::action_name`] for a module-specific
/// verb that does not fit one of the canonical variants.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventAction {
    Deposit,
    Withdraw,
    Borrow,
    Repay,
    Liquidate,
    PriceUpdate,
    ParamsUpdate,
    Pause,
    Unpause,
    ProposalCreated,
    VoteCast,
    Execute,
    Claim,
    FlashLoan,
    Other,
}

impl EventAction {
    /// Stable snake_case name for this action. Used as
    /// [`StructuredEventV1::action_name`] unless the caller overrides it.
    pub fn as_symbol(&self, e: &Env) -> Symbol {
        match self {
            EventAction::Deposit => Symbol::new(e, "deposit"),
            EventAction::Withdraw => Symbol::new(e, "withdraw"),
            EventAction::Borrow => Symbol::new(e, "borrow"),
            EventAction::Repay => Symbol::new(e, "repay"),
            EventAction::Liquidate => Symbol::new(e, "liquidate"),
            EventAction::PriceUpdate => Symbol::new(e, "price_update"),
            EventAction::ParamsUpdate => Symbol::new(e, "params_update"),
            EventAction::Pause => Symbol::new(e, "pause"),
            EventAction::Unpause => Symbol::new(e, "unpause"),
            EventAction::ProposalCreated => Symbol::new(e, "proposal_created"),
            EventAction::VoteCast => Symbol::new(e, "vote_cast"),
            EventAction::Execute => Symbol::new(e, "execute"),
            EventAction::Claim => Symbol::new(e, "claim"),
            EventAction::FlashLoan => Symbol::new(e, "flash_loan"),
            EventAction::Other => Symbol::new(e, "other"),
        }
    }
}

/// A single key/value numeric annotation attached to a [`StructuredEventV1`],
/// e.g. `{ key: "health_factor", value: 1_050 }`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuredEventField {
    pub key: Symbol,
    pub value: i128,
}

/// Versioned, self-describing envelope for a single protocol state change.
///
/// # Topic layout
///
/// `("proto_evt", module, action, actor)` — a fixed prefix plus the three
/// indexed fields. All remaining fields are in the data payload.
#[contractevent(topics = ["proto_evt"])]
#[derive(Clone, Debug)]
pub struct StructuredEventV1 {
    #[topic]
    pub module: EventModule,
    #[topic]
    pub action: EventAction,
    #[topic]
    pub actor: Address,
    /// Schema version this payload conforms to ([`EVENT_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Concrete verb; equals `action.as_symbol()` unless `action` is `Other`.
    pub action_name: Symbol,
    /// Primary asset the event concerns (`None` = native XLM / not applicable).
    pub asset: Option<Address>,
    /// Primary signed amount in base units (`0` when not applicable).
    pub amount: i128,
    /// Secondary party (borrower, delegatee, recipient…), if any.
    pub counterparty: Option<Address>,
    /// Structured numeric annotations.
    pub metadata: Vec<StructuredEventField>,
    /// Ledger timestamp captured at emission.
    pub timestamp: u64,
}

/// Ergonomic builder for [`StructuredEventV1`].
///
/// ```ignore
/// StructuredEvent::new(env, EventModule::Lending, EventAction::Borrow, user.clone())
///     .with_asset(asset)
///     .with_amount(amount)
///     .with_meta(env, "health_factor", hf)
///     .emit(env);
/// ```
pub struct StructuredEvent {
    module: EventModule,
    action: EventAction,
    actor: Address,
    action_name: Symbol,
    asset: Option<Address>,
    amount: i128,
    counterparty: Option<Address>,
    metadata: Vec<StructuredEventField>,
}

impl StructuredEvent {
    /// Start a new envelope. `action_name` defaults to `action.as_symbol()`.
    pub fn new(e: &Env, module: EventModule, action: EventAction, actor: Address) -> Self {
        Self {
            action_name: action.as_symbol(e),
            module,
            action,
            actor,
            asset: None,
            amount: 0,
            counterparty: None,
            metadata: Vec::new(e),
        }
    }

    /// Override the concrete verb. Required when `action` is
    /// [`EventAction::Other`]; otherwise optional.
    pub fn with_name(mut self, e: &Env, name: &str) -> Self {
        self.action_name = Symbol::new(e, name);
        self
    }

    /// Set the primary asset (`None` = native XLM).
    pub fn with_asset(mut self, asset: Option<Address>) -> Self {
        self.asset = asset;
        self
    }

    /// Set the primary signed amount in base units.
    pub fn with_amount(mut self, amount: i128) -> Self {
        self.amount = amount;
        self
    }

    /// Set the secondary counterparty address.
    pub fn with_counterparty(mut self, counterparty: Address) -> Self {
        self.counterparty = Some(counterparty);
        self
    }

    /// Append a numeric annotation. Chainable for multiple fields.
    pub fn with_meta(mut self, e: &Env, key: &str, value: i128) -> Self {
        self.metadata.push_back(StructuredEventField {
            key: Symbol::new(e, key),
            value,
        });
        self
    }

    /// Materialize the envelope and publish it. The timestamp is read from the
    /// current ledger.
    pub fn emit(self, e: &Env) {
        StructuredEventV1 {
            module: self.module,
            action: self.action,
            actor: self.actor,
            schema_version: EVENT_SCHEMA_VERSION,
            action_name: self.action_name,
            asset: self.asset,
            amount: self.amount,
            counterparty: self.counterparty,
            metadata: self.metadata,
            timestamp: e.ledger().timestamp(),
        }
        .publish(e);
    }
}

/// Free-function form for callers that have already constructed the envelope.
pub fn emit_structured(e: &Env, event: StructuredEventV1) {
    event.publish(e);
}
