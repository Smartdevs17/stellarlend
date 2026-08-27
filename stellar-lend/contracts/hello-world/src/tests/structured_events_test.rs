//! # Structured Event Schema – Tests (Issue #824)
//!
//! Verifies the additive [`StructuredEventV1`] envelope:
//!   - the builder populates every field correctly,
//!   - defaults are applied (action name, empty metadata, zero amount),
//!   - the ledger timestamp is captured at emission,
//!   - topics follow the documented `("proto_evt", module, action, actor)` layout,
//!   - `emit_structured` publishes a pre-built envelope unchanged,
//!   - emitting the structured envelope is independent of the typed emitters
//!     (no regression to existing `emit_*` helpers).

use crate::events::{
    emit_deposit, emit_structured, EventAction, EventModule, StructuredEvent, StructuredEventField,
    StructuredEventV1, DepositEvent, EVENT_SCHEMA_VERSION,
};
use crate::HelloContract;

use soroban_sdk::{
    contracttype,
    testutils::{Address as _, Events, Ledger},
    Address, Env, Symbol, TryFromVal, Vec,
};

/// Mirror of [`StructuredEventV1`] for decoding the event data payload.
#[contracttype]
#[derive(Clone, Debug)]
pub struct TestStructuredEvent {
    pub module: EventModule,
    pub action: EventAction,
    pub actor: Address,
    pub schema_version: u32,
    pub action_name: Symbol,
    pub asset: Option<Address>,
    pub amount: i128,
    pub counterparty: Option<Address>,
    pub metadata: Vec<StructuredEventField>,
    pub timestamp: u64,
}

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(HelloContract, ());
    (env, contract_id)
}

fn decode(env: &Env) -> (soroban_sdk::Vec<soroban_sdk::Val>, TestStructuredEvent) {
    let all = env.events().all();
    assert_eq!(all.len(), 1, "expected exactly one event");
    let (_contract, topics, data) = all.get_unchecked(0);
    let decoded = TestStructuredEvent::try_from_val(env, &data)
        .expect("failed to decode StructuredEventV1 payload");
    (topics, decoded)
}

#[test]
fn schema_version_is_one() {
    assert_eq!(EVENT_SCHEMA_VERSION, 1);
}

#[test]
fn action_symbols_are_stable_snake_case() {
    let env = Env::default();
    assert_eq!(EventAction::Deposit.as_symbol(&env), Symbol::new(&env, "deposit"));
    assert_eq!(EventAction::PriceUpdate.as_symbol(&env), Symbol::new(&env, "price_update"));
    assert_eq!(
        EventAction::ProposalCreated.as_symbol(&env),
        Symbol::new(&env, "proposal_created")
    );
    assert_eq!(EventAction::Other.as_symbol(&env), Symbol::new(&env, "other"));
}

#[test]
fn builder_emits_envelope_with_defaults() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_234);

    env.as_contract(&contract_id, || {
        let user = Address::generate(&env);
        StructuredEvent::new(&env, EventModule::Lending, EventAction::Deposit, user.clone())
            .emit(&env);

        let (topics, decoded) = decode(&env);

        assert_eq!(decoded.module, EventModule::Lending);
        assert_eq!(decoded.action, EventAction::Deposit);
        assert_eq!(decoded.actor, user);
        assert_eq!(decoded.schema_version, EVENT_SCHEMA_VERSION);
        assert_eq!(decoded.action_name, Symbol::new(&env, "deposit"));
        assert_eq!(decoded.asset, None);
        assert_eq!(decoded.amount, 0);
        assert_eq!(decoded.counterparty, None);
        assert_eq!(decoded.metadata.len(), 0);
        assert_eq!(decoded.timestamp, 1_234);

        // Topic layout: ("proto_evt", module, action, actor)
        assert!(topics.len() >= 4, "expected at least 4 topics");
        let prefix = Symbol::try_from_val(&env, &topics.get_unchecked(0))
            .expect("first topic is a Symbol");
        assert_eq!(prefix, Symbol::new(&env, "proto_evt"));
    });
}

#[test]
fn builder_populates_all_fields() {
    let (env, contract_id) = setup();
    env.ledger().set_timestamp(9_000);

    env.as_contract(&contract_id, || {
        let liquidator = Address::generate(&env);
        let borrower = Address::generate(&env);
        let asset = Address::generate(&env);

        StructuredEvent::new(
            &env,
            EventModule::Liquidation,
            EventAction::Liquidate,
            liquidator.clone(),
        )
        .with_asset(Some(asset.clone()))
        .with_amount(-500)
        .with_counterparty(borrower.clone())
        .with_meta(&env, "health_factor", 1_050)
        .with_meta(&env, "seized", 750)
        .emit(&env);

        let (_topics, decoded) = decode(&env);
        assert_eq!(decoded.module, EventModule::Liquidation);
        assert_eq!(decoded.action, EventAction::Liquidate);
        assert_eq!(decoded.actor, liquidator);
        assert_eq!(decoded.asset, Some(asset));
        assert_eq!(decoded.amount, -500);
        assert_eq!(decoded.counterparty, Some(borrower));
        assert_eq!(decoded.timestamp, 9_000);
        assert_eq!(decoded.metadata.len(), 2);
        let first = decoded.metadata.get_unchecked(0);
        assert_eq!(first.key, Symbol::new(&env, "health_factor"));
        assert_eq!(first.value, 1_050);
        let second = decoded.metadata.get_unchecked(1);
        assert_eq!(second.key, Symbol::new(&env, "seized"));
        assert_eq!(second.value, 750);
    });
}

#[test]
fn with_name_overrides_action_name_for_other() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let actor = Address::generate(&env);
        StructuredEvent::new(&env, EventModule::Governance, EventAction::Other, actor)
            .with_name(&env, "delegate_votes")
            .emit(&env);

        let (_topics, decoded) = decode(&env);
        assert_eq!(decoded.action, EventAction::Other);
        assert_eq!(decoded.action_name, Symbol::new(&env, "delegate_votes"));
    });
}

#[test]
fn emit_structured_publishes_prebuilt_envelope_unchanged() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let actor = Address::generate(&env);
        let envelope = StructuredEventV1 {
            module: EventModule::Oracle,
            action: EventAction::PriceUpdate,
            actor: actor.clone(),
            schema_version: EVENT_SCHEMA_VERSION,
            action_name: Symbol::new(&env, "price_update"),
            asset: None,
            amount: 42,
            counterparty: None,
            metadata: Vec::new(&env),
            timestamp: 77,
        };
        emit_structured(&env, envelope);

        let (_topics, decoded) = decode(&env);
        assert_eq!(decoded.module, EventModule::Oracle);
        assert_eq!(decoded.action, EventAction::PriceUpdate);
        assert_eq!(decoded.amount, 42);
        assert_eq!(decoded.timestamp, 77);
    });
}

#[test]
fn structured_envelope_is_independent_of_typed_emitters() {
    let (env, contract_id) = setup();

    env.as_contract(&contract_id, || {
        let user = Address::generate(&env);

        // A typed emitter still produces exactly one event on its own.
        emit_deposit(
            &env,
            DepositEvent { user: user.clone(), asset: None, amount: 10, timestamp: 1 },
        );
        assert_eq!(env.events().all().len(), 1);

        // The structured envelope adds one more, opt-in.
        StructuredEvent::new(&env, EventModule::Lending, EventAction::Deposit, user)
            .with_amount(10)
            .emit(&env);
        assert_eq!(env.events().all().len(), 2);
    });
}
