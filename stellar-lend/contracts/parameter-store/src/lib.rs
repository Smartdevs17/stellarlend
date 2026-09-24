#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

pub mod hello_world_bridge;
pub mod simulation;
pub mod voting;

pub use simulation::{ImpactSeverity, ParameterImpact, PoolSnapshot, RelatedParameters};
pub use voting::{ParameterVote, VoteTally, VotingConfig};

pub const BPS_DIVISOR: i128 = 10_000;
pub const RISK_TIMELOCK_SECONDS: u64 = 48 * 3600;
pub const STANDARD_TIMELOCK_SECONDS: u64 = 24 * 3600;
pub const EMERGENCY_TIMELOCK_SECONDS: u64 = 4 * 3600;

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum ParameterType {
    LTV,
    LiquidationThreshold,
    CloseFactor,
    LiquidationIncentive,
    ReserveFactor,
    DebtCeiling,
    BaseInterestRate,
    Slope1,
    Slope2,
    OptimalUtilization,
}

impl ParameterType {
    pub fn min_timelock(&self) -> u64 {
        match self {
            ParameterType::LTV
            | ParameterType::LiquidationThreshold
            | ParameterType::CloseFactor
            | ParameterType::LiquidationIncentive => RISK_TIMELOCK_SECONDS,
            _ => STANDARD_TIMELOCK_SECONDS,
        }
    }

    pub fn is_risk_parameter(&self) -> bool {
        matches!(
            self,
            ParameterType::LTV
                | ParameterType::LiquidationThreshold
                | ParameterType::CloseFactor
                | ParameterType::LiquidationIncentive
        )
    }

    pub fn validate_range(&self, value: i128) -> bool {
        match self {
            ParameterType::LTV => value > 0 && value <= 9_000,
            ParameterType::LiquidationThreshold => value > 0 && value <= BPS_DIVISOR,
            ParameterType::CloseFactor => value > 0 && value <= BPS_DIVISOR,
            ParameterType::LiquidationIncentive => value >= 1_000 && value <= 2_000,
            ParameterType::ReserveFactor => value >= 0 && value <= BPS_DIVISOR,
            ParameterType::DebtCeiling => value >= 0,
            ParameterType::BaseInterestRate => value >= 0 && value <= 5_000,
            ParameterType::Slope1 => value >= 0 && value <= BPS_DIVISOR,
            ParameterType::Slope2 => value >= 0 && value <= 50_000,
            ParameterType::OptimalUtilization => value > 0 && value < BPS_DIVISOR,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ParameterChange {
    pub parameter: ParameterType,
    pub old_value: i128,
    pub new_value: i128,
    pub timestamp: u64,
    pub effective_at: u64,
    pub changed_by: Address,
    /// Monotonic version of this parameter for this pool, starting at 1.
    ///
    /// Every accepted change mints a new version, and the value at each version
    /// stays readable through
    /// [`ParameterStoreContract::get_parameter_at_version`], so an audit can
    /// reconstruct exactly what the pool was configured with at any point.
    pub version: u32,
}

/// Payload published on every parameter change, for off-chain subscribers.
///
/// Emitted under the `param_changed` topic alongside the parameter type, so an
/// indexer can filter by parameter without decoding every event body.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ParameterChangeNotification {
    pub pool: Address,
    pub parameter: ParameterType,
    pub old_value: i128,
    pub new_value: i128,
    pub version: u32,
    pub effective_at: u64,
    /// `true` when the change came through the emergency override path.
    pub is_emergency: bool,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ParameterProposal {
    pub id: u64,
    pub pool: Address,
    pub parameter: ParameterType,
    pub proposed_value: i128,
    pub proposer: Address,
    pub created_at: u64,
    pub effective_at: u64,
    pub accepted: bool,
    pub rejected: bool,
    pub is_emergency: bool,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct PoolRegistration {
    pub pool: Address,
    pub registered_at: u64,
    pub active: bool,
}

#[contract]
pub struct ParameterStoreContract;

#[contractimpl]
impl ParameterStoreContract {
    pub fn initialize(env: Env, governance: Address, admin: Address) {
        if env.storage().instance().has(&DataKey::Governance) {
            panic!("Already initialized");
        }
        governance.require_auth();
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Governance, &governance);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &0u64);
        env.storage().instance().set(&DataKey::PoolCounter, &0u64);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyOverrideActive, &false);
    }

    pub fn register_pool(env: Env, pool: Address) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PoolCounter)
            .unwrap_or(0);
        let pool_id = counter + 1;
        let registration = PoolRegistration {
            pool: pool.clone(),
            registered_at: env.ledger().timestamp(),
            active: true,
        };
        env.storage()
            .instance()
            .set(&DataKey::Pool(pool_id), &registration);
        env.storage()
            .instance()
            .set(&DataKey::PoolAddress(pool.clone()), &pool_id);
        env.storage()
            .instance()
            .set(&DataKey::PoolCounter, &pool_id);
        env.events().publish(("register_pool",), &pool);
    }

    pub fn propose_change(
        env: Env,
        pool: Address,
        parameter: ParameterType,
        value: i128,
        timelock_seconds: u64,
    ) -> u64 {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        assert!(
            env.storage()
                .instance()
                .has(&DataKey::PoolAddress(pool.clone())),
            "Pool not registered"
        );
        assert!(
            parameter.validate_range(value),
            "Parameter value out of range"
        );

        let min_timelock = parameter.min_timelock();
        assert!(
            timelock_seconds >= min_timelock,
            "Timelock too short: min {min_timelock}s"
        );

        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCounter)
            .unwrap_or(0);
        let proposal_id = counter + 1;
        let current_timestamp = env.ledger().timestamp();
        let effective_at = current_timestamp + timelock_seconds;

        let param_clone = parameter.clone();
        let proposal = ParameterProposal {
            id: proposal_id,
            pool: pool.clone(),
            parameter,
            proposed_value: value,
            proposer: governance.clone(),
            created_at: current_timestamp,
            effective_at,
            accepted: false,
            rejected: false,
            is_emergency: false,
        };

        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &proposal_id);
        env.events()
            .publish(("propose_change", &param_clone), &proposal_id);

        proposal_id
    }

    pub fn propose_emergency_change(
        env: Env,
        pool: Address,
        parameter: ParameterType,
        value: i128,
    ) -> u64 {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        assert!(
            parameter.is_risk_parameter(),
            "Emergency override only for risk params"
        );
        assert!(
            env.storage()
                .instance()
                .has(&DataKey::PoolAddress(pool.clone())),
            "Pool not registered"
        );
        assert!(
            parameter.validate_range(value),
            "Parameter value out of range"
        );

        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCounter)
            .unwrap_or(0);
        let proposal_id = counter + 1;
        let current_timestamp = env.ledger().timestamp();
        let effective_at = current_timestamp + EMERGENCY_TIMELOCK_SECONDS;

        let param_clone = parameter.clone();
        let proposal = ParameterProposal {
            id: proposal_id,
            pool: pool.clone(),
            parameter,
            proposed_value: value,
            proposer: governance.clone(),
            created_at: current_timestamp,
            effective_at,
            accepted: false,
            rejected: false,
            is_emergency: true,
        };

        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &proposal_id);
        env.events()
            .publish(("propose_emergency", &param_clone), &proposal_id);

        proposal_id
    }

    pub fn accept_proposal(env: Env, proposal_id: u64) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let mut proposal: ParameterProposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found");

        let current_timestamp = env.ledger().timestamp();
        assert!(
            current_timestamp >= proposal.effective_at,
            "Timelock not elapsed"
        );
        assert!(
            !proposal.accepted && !proposal.rejected,
            "Proposal already decided"
        );

        // When governance has installed voting rules, the vote decides; until
        // then the governance address accepts directly, as it always has.
        if let Some(config) = voting_config(&env) {
            assert!(
                !voting::is_voting_open(&env, proposal.created_at, &config),
                "Voting still open"
            );
            let tally = build_tally(&env, proposal_id);
            assert!(tally.has_passed(&config), "Proposal did not pass the vote");
        }

        proposal.accepted = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        commit_change(&env, &proposal, current_timestamp, false);

        env.events()
            .publish(("accept_proposal", &proposal.parameter), &proposal_id);
    }

    pub fn execute_emergency_override(env: Env, proposal_id: u64) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();

        let mut proposal: ParameterProposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found");

        assert!(proposal.is_emergency, "Not an emergency proposal");
        assert!(
            !proposal.accepted && !proposal.rejected,
            "Proposal already decided"
        );

        let current_timestamp = env.ledger().timestamp();
        assert!(
            current_timestamp >= proposal.effective_at,
            "Emergency timelock not elapsed"
        );

        proposal.accepted = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        commit_change(&env, &proposal, current_timestamp, true);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyOverrideActive, &true);

        env.events().publish(("emergency_override",), &proposal_id);
    }

    pub fn clear_emergency_override(env: Env) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::EmergencyOverrideActive, &false);
        env.events().publish(("clear_emergency",), &());
    }

    pub fn reject_proposal(env: Env, proposal_id: u64) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();
        let mut proposal: ParameterProposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found");
        assert!(
            !proposal.accepted && !proposal.rejected,
            "Proposal already decided"
        );
        proposal.rejected = true;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.events().publish(("reject_proposal",), &proposal_id);
    }

    pub fn get_parameter(env: Env, parameter: ParameterType, pool: Address) -> i128 {
        let _pool_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PoolAddress(pool.clone()))
            .expect("Pool not registered");
        let key = DataKey::Parameter(parameter, pool.clone());
        env.storage().instance().get(&key).unwrap_or(0)
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> ParameterProposal {
        env.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found")
    }

    pub fn get_change_history(
        env: Env,
        parameter: ParameterType,
        pool: Address,
    ) -> Vec<ParameterChange> {
        let key = DataKey::ChangeHistory(parameter, pool);
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn is_emergency_active(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::EmergencyOverrideActive)
            .unwrap_or(false)
    }

    // ---------------------------------------------------------------- versioning

    /// Current version of a parameter for a pool. 0 means never set.
    pub fn get_parameter_version(env: Env, parameter: ParameterType, pool: Address) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ParameterVersion(parameter, pool))
            .unwrap_or(0)
    }

    /// The value a parameter held at a given version.
    ///
    /// Panics for a version that was never minted, rather than returning a
    /// default that a caller could mistake for a real historical value.
    pub fn get_parameter_at_version(
        env: Env,
        parameter: ParameterType,
        pool: Address,
        version: u32,
    ) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::VersionedValue(parameter, pool, version))
            .expect("Version not found")
    }

    // ------------------------------------------------------------------- voting

    /// Installs or replaces the voting rules.
    ///
    /// Until this is called, proposals are accepted directly by governance.
    pub fn set_voting_config(env: Env, config: VotingConfig) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();
        assert!(config.is_valid(), "Invalid voting config");
        env.storage().instance().set(&DataKey::VotingConfig, &config);
        env.events().publish(("voting_config",), &config);
    }

    /// Reads the installed voting rules, if any.
    pub fn get_voting_config(env: Env) -> Option<VotingConfig> {
        env.storage().instance().get(&DataKey::VotingConfig)
    }

    /// Sets an address's voting weight, keeping the registered total in step.
    ///
    /// Setting a weight to 0 removes the voter from future quorum maths;
    /// votes they already cast stand, since a tally reflects the weight held
    /// when the vote was cast.
    pub fn set_voting_power(env: Env, voter: Address, weight: i128) {
        let governance: Address = env.storage().instance().get(&DataKey::Governance).unwrap();
        governance.require_auth();
        assert!(weight >= 0, "Voting weight cannot be negative");

        let previous: i128 = env
            .storage()
            .instance()
            .get(&DataKey::VotingPower(voter.clone()))
            .unwrap_or(0);
        let total: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalVotingPower)
            .unwrap_or(0);

        env.storage()
            .instance()
            .set(&DataKey::VotingPower(voter.clone()), &weight);
        env.storage().instance().set(
            &DataKey::TotalVotingPower,
            &(total - previous + weight),
        );
        env.events().publish(("voting_power", &voter), &weight);
    }

    /// Voting weight registered for an address.
    pub fn get_voting_power(env: Env, voter: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::VotingPower(voter))
            .unwrap_or(0)
    }

    /// Total registered voting power, the denominator for quorum.
    pub fn get_total_voting_power(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalVotingPower)
            .unwrap_or(0)
    }

    /// Casts a vote on a proposal.
    ///
    /// Requires the voter's own authorization — governance registers weight but
    /// cannot vote on a holder's behalf. One vote per address per proposal;
    /// changing a vote is not supported, so a voter cannot wait out the window
    /// and flip the result at the last moment.
    pub fn cast_vote(env: Env, proposal_id: u64, voter: Address, support: bool) {
        voter.require_auth();

        let config = voting_config(&env).expect("Voting is not enabled");
        let proposal: ParameterProposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found");
        assert!(
            !proposal.accepted && !proposal.rejected,
            "Proposal already decided"
        );
        assert!(
            voting::is_voting_open(&env, proposal.created_at, &config),
            "Voting closed"
        );

        let weight: i128 = env
            .storage()
            .instance()
            .get(&DataKey::VotingPower(voter.clone()))
            .unwrap_or(0);
        assert!(weight > 0, "No voting power");

        let mut votes = stored_votes(&env, proposal_id);
        assert!(!voting::has_voted(&votes, &voter), "Already voted");

        votes.push_back(ParameterVote {
            proposal_id,
            voter: voter.clone(),
            support,
            weight,
            voted_at: env.ledger().timestamp(),
        });
        env.storage()
            .instance()
            .set(&DataKey::Votes(proposal_id), &votes);

        env.events()
            .publish(("cast_vote", proposal_id), (voter, support, weight));
    }

    /// Current tally for a proposal.
    pub fn get_vote_tally(env: Env, proposal_id: u64) -> VoteTally {
        build_tally(&env, proposal_id)
    }

    /// Every vote cast on a proposal.
    pub fn get_votes(env: Env, proposal_id: u64) -> Vec<ParameterVote> {
        stored_votes(&env, proposal_id)
    }

    /// Whether a proposal has cleared quorum and the approval threshold.
    ///
    /// Returns `true` when voting is not enabled, matching the acceptance path.
    pub fn has_proposal_passed(env: Env, proposal_id: u64) -> bool {
        match voting_config(&env) {
            None => true,
            Some(config) => build_tally(&env, proposal_id).has_passed(&config),
        }
    }

    // --------------------------------------------------------------- simulation

    /// Projects the effect of a proposed value against a snapshot of pool state.
    ///
    /// Read-only and pure in its inputs: the same snapshot always produces the
    /// same projection, so the API and a voter see identical numbers.
    pub fn simulate_change(
        env: Env,
        pool: Address,
        parameter: ParameterType,
        proposed_value: i128,
        snapshot: PoolSnapshot,
    ) -> ParameterImpact {
        let current = read_parameter(&env, &parameter, &pool);
        let related = related_parameters(&env, &pool);
        simulation::simulate(
            &env,
            &parameter,
            current,
            proposed_value,
            &snapshot,
            &related,
        )
    }

    /// Projects the effect of an existing proposal.
    pub fn simulate_proposal(env: Env, proposal_id: u64, snapshot: PoolSnapshot) -> ParameterImpact {
        let proposal: ParameterProposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found");
        let current = read_parameter(&env, &proposal.parameter, &proposal.pool);
        let related = related_parameters(&env, &proposal.pool);
        simulation::simulate(
            &env,
            &proposal.parameter,
            current,
            proposal.proposed_value,
            &snapshot,
            &related,
        )
    }

    /// Validates a value against the parameter's own range **and** against the
    /// other parameters already set for the pool.
    ///
    /// Range validation alone cannot catch an LTV of 80% against a liquidation
    /// threshold of 75%: both are individually legal, together they let a
    /// borrower open a position that is instantly liquidatable.
    pub fn validate_value(env: Env, pool: Address, parameter: ParameterType, value: i128) -> bool {
        if !parameter.validate_range(value) {
            return false;
        }
        let related = related_parameters(&env, &pool);
        match parameter {
            ParameterType::LTV => {
                related.liquidation_threshold == 0 || value < related.liquidation_threshold
            }
            ParameterType::LiquidationThreshold => related.ltv == 0 || value > related.ltv,
            ParameterType::OptimalUtilization => value > 0 && value < BPS_DIVISOR,
            _ => true,
        }
    }
}

/// Writes an accepted proposal's value through: current value, new version,
/// audit trail, and the change notification.
///
/// Shared by the ordinary and emergency acceptance paths so the two can never
/// record a change differently.
fn commit_change(env: &Env, proposal: &ParameterProposal, timestamp: u64, is_emergency: bool) {
    let key = DataKey::Parameter(proposal.parameter.clone(), proposal.pool.clone());
    let old_value: i128 = env.storage().instance().get(&key).unwrap_or(0);

    let version_key = DataKey::ParameterVersion(proposal.parameter.clone(), proposal.pool.clone());
    let version: u32 = env.storage().instance().get(&version_key).unwrap_or(0) + 1;

    env.storage().instance().set(&key, &proposal.proposed_value);
    env.storage().instance().set(&version_key, &version);
    env.storage().instance().set(
        &DataKey::VersionedValue(proposal.parameter.clone(), proposal.pool.clone(), version),
        &proposal.proposed_value,
    );

    let change = ParameterChange {
        parameter: proposal.parameter.clone(),
        old_value,
        new_value: proposal.proposed_value,
        timestamp,
        effective_at: proposal.effective_at,
        changed_by: proposal.proposer.clone(),
        version,
    };

    let history_key = DataKey::ChangeHistory(proposal.parameter.clone(), proposal.pool.clone());
    let mut history: Vec<ParameterChange> = env
        .storage()
        .instance()
        .get(&history_key)
        .unwrap_or_else(|| Vec::new(env));
    history.push_back(change);
    env.storage().instance().set(&history_key, &history);

    let notification = ParameterChangeNotification {
        pool: proposal.pool.clone(),
        parameter: proposal.parameter.clone(),
        old_value,
        new_value: proposal.proposed_value,
        version,
        effective_at: proposal.effective_at,
        is_emergency,
    };
    env.events().publish(
        (Symbol::new(env, "param_changed"), proposal.parameter.clone()),
        notification,
    );
}

fn voting_config(env: &Env) -> Option<VotingConfig> {
    env.storage().instance().get(&DataKey::VotingConfig)
}

fn stored_votes(env: &Env, proposal_id: u64) -> Vec<ParameterVote> {
    env.storage()
        .instance()
        .get(&DataKey::Votes(proposal_id))
        .unwrap_or_else(|| Vec::new(env))
}

fn build_tally(env: &Env, proposal_id: u64) -> VoteTally {
    let votes = stored_votes(env, proposal_id);
    let total: i128 = env
        .storage()
        .instance()
        .get(&DataKey::TotalVotingPower)
        .unwrap_or(0);
    voting::tally_votes(proposal_id, &votes, total)
}

/// Reads a parameter without requiring the pool to be registered, for the
/// read-only paths (simulation, validation) that must not panic on a pool that
/// has not been set up yet.
fn read_parameter(env: &Env, parameter: &ParameterType, pool: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::Parameter(parameter.clone(), pool.clone()))
        .unwrap_or(0)
}

/// Gathers the parameter values that cross-checks and rate projections need.
fn related_parameters(env: &Env, pool: &Address) -> RelatedParameters {
    RelatedParameters {
        ltv: read_parameter(env, &ParameterType::LTV, pool),
        liquidation_threshold: read_parameter(env, &ParameterType::LiquidationThreshold, pool),
        base_interest_rate: read_parameter(env, &ParameterType::BaseInterestRate, pool),
        slope1: read_parameter(env, &ParameterType::Slope1, pool),
        slope2: read_parameter(env, &ParameterType::Slope2, pool),
        optimal_utilization: read_parameter(env, &ParameterType::OptimalUtilization, pool),
    }
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Governance,
    Admin,
    ProposalCounter,
    PoolCounter,
    Pool(u64),
    PoolAddress(Address),
    Proposal(u64),
    Parameter(ParameterType, Address),
    ChangeHistory(ParameterType, Address),
    EmergencyOverrideActive,
    /// Current version number of a parameter for a pool.
    ParameterVersion(ParameterType, Address),
    /// Value a parameter held at a specific version.
    VersionedValue(ParameterType, Address, u32),
    /// Voting rules, absent until governance installs them.
    VotingConfig,
    /// Voting weight registered for an address.
    VotingPower(Address),
    /// Sum of all registered voting power.
    TotalVotingPower,
    /// Votes cast on a proposal.
    Votes(u64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke};
    use soroban_sdk::{IntoVal, Vec};

    struct TestEnv {
        env: Env,
        contract_id: Address,
        governance: Address,
        admin: Address,
    }

    fn setup() -> TestEnv {
        let env = Env::default();
        env.mock_all_auths();
        let governance = Address::generate(&env);
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, ParameterStoreContract);
        let client = ParameterStoreContractClient::new(&env, &contract_id);
        client.initialize(&governance, &admin);
        TestEnv {
            env,
            contract_id,
            governance,
            admin,
        }
    }

    fn with_governance_auth<T>(
        te: &TestEnv,
        fn_name: &str,
        args: impl IntoVal<Env, Vec<soroban_sdk::Val>>,
        f: impl FnOnce() -> T,
    ) -> T {
        te.env.mock_auths(&[MockAuth {
            address: &te.governance,
            invoke: &MockAuthInvoke {
                contract: &te.contract_id,
                fn_name,
                args: args.into_val(&te.env),
                sub_invokes: &[],
            },
        }]);
        f()
    }

    fn client(te: &TestEnv) -> ParameterStoreContractClient<'_> {
        ParameterStoreContractClient::new(&te.env, &te.contract_id)
    }

    #[test]
    fn test_initialize() {
        let te = setup();
        let stored: Address = te
            .env
            .as_contract(&te.contract_id, || {
                te.env.storage().instance().get(&DataKey::Governance)
            })
            .unwrap();
        assert_eq!(stored, te.governance);
    }

    #[test]
    fn initialize_requires_governance_auth() {
        let env = Env::default();
        let governance = Address::generate(&env);
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, ParameterStoreContract);
        let client = ParameterStoreContractClient::new(&env, &contract_id);

        assert!(client.try_initialize(&governance, &admin).is_err());
    }

    #[test]
    fn initialize_cannot_overwrite_governance() {
        let te = setup();
        let attacker = Address::generate(&te.env);

        assert!(client(&te).try_initialize(&attacker, &attacker).is_err());

        let stored: Address = te
            .env
            .as_contract(&te.contract_id, || {
                te.env.storage().instance().get(&DataKey::Governance)
            })
            .unwrap();
        assert_eq!(stored, te.governance);
    }

    #[test]
    fn test_register_pool() {
        let te = setup();
        let pool = Address::generate(&te.env);
        with_governance_auth(&te, "register_pool", (&pool,), || {
            client(&te).register_pool(&pool);
        });
        let stored: u64 = te
            .env
            .as_contract(&te.contract_id, || {
                te.env.storage().instance().get(&DataKey::PoolCounter)
            })
            .unwrap();
        assert_eq!(stored, 1);
    }

    #[test]
    fn test_propose_and_accept_change() {
        let te = setup();
        let pool = Address::generate(&te.env);
        with_governance_auth(&te, "register_pool", (&pool,), || {
            client(&te).register_pool(&pool);
        });

        with_governance_auth(
            &te,
            "propose_change",
            (
                &pool,
                &ParameterType::LiquidationThreshold,
                &8_000i128,
                &RISK_TIMELOCK_SECONDS,
            ),
            || {
                let proposal_id = client(&te).propose_change(
                    &pool,
                    &ParameterType::LiquidationThreshold,
                    &8_000,
                    &RISK_TIMELOCK_SECONDS,
                );
                assert_eq!(proposal_id, 1);
            },
        );

        te.env.ledger().set_timestamp(RISK_TIMELOCK_SECONDS + 1);

        with_governance_auth(&te, "accept_proposal", (&1u64,), || {
            client(&te).accept_proposal(&1);
        });

        let history = client(&te).get_change_history(&ParameterType::LiquidationThreshold, &pool);
        assert_eq!(history.len(), 1);
    }

    #[test]
    #[should_panic(expected = "Timelock too short")]
    fn test_propose_timelock_too_short() {
        let te = setup();
        let pool = Address::generate(&te.env);
        with_governance_auth(&te, "register_pool", (&pool,), || {
            client(&te).register_pool(&pool);
        });
        with_governance_auth(
            &te,
            "propose_change",
            (&pool, &ParameterType::LTV, &7_000i128, &3600u64),
            || {
                client(&te).propose_change(&pool, &ParameterType::LTV, &7_000, &3600);
            },
        );
    }

    #[test]
    #[should_panic(expected = "Timelock not elapsed")]
    fn test_accept_before_timelock() {
        let te = setup();
        let pool = Address::generate(&te.env);
        with_governance_auth(&te, "register_pool", (&pool,), || {
            client(&te).register_pool(&pool);
        });
        with_governance_auth(
            &te,
            "propose_change",
            (
                &pool,
                &ParameterType::LTV,
                &7_000i128,
                &RISK_TIMELOCK_SECONDS,
            ),
            || {
                client(&te).propose_change(
                    &pool,
                    &ParameterType::LTV,
                    &7_000,
                    &RISK_TIMELOCK_SECONDS,
                );
            },
        );
        with_governance_auth(&te, "accept_proposal", (&1u64,), || {
            client(&te).accept_proposal(&1);
        });
    }

    #[test]
    #[should_panic(expected = "Parameter value out of range")]
    fn test_ltv_out_of_range() {
        let te = setup();
        let pool = Address::generate(&te.env);
        with_governance_auth(&te, "register_pool", (&pool,), || {
            client(&te).register_pool(&pool);
        });
        with_governance_auth(
            &te,
            "propose_change",
            (
                &pool,
                &ParameterType::LTV,
                &9_500i128,
                &RISK_TIMELOCK_SECONDS,
            ),
            || {
                client(&te).propose_change(
                    &pool,
                    &ParameterType::LTV,
                    &9_500,
                    &RISK_TIMELOCK_SECONDS,
                );
            },
        );
    }

    #[test]
    fn test_emergency_proposal() {
        let te = setup();
        let pool = Address::generate(&te.env);
        with_governance_auth(&te, "register_pool", (&pool,), || {
            client(&te).register_pool(&pool);
        });
        with_governance_auth(
            &te,
            "propose_emergency_change",
            (&pool, &ParameterType::LiquidationThreshold, &7_500i128),
            || {
                client(&te).propose_emergency_change(
                    &pool,
                    &ParameterType::LiquidationThreshold,
                    &7_500,
                );
            },
        );

        te.env
            .ledger()
            .set_timestamp(EMERGENCY_TIMELOCK_SECONDS + 1);

        with_governance_auth(&te, "execute_emergency_override", (&1u64,), || {
            client(&te).execute_emergency_override(&1);
        });

        assert!(client(&te).is_emergency_active());
    }

    #[test]
    fn test_clear_emergency_override() {
        let te = setup();
        let pool = Address::generate(&te.env);
        with_governance_auth(&te, "register_pool", (&pool,), || {
            client(&te).register_pool(&pool);
        });
        with_governance_auth(
            &te,
            "propose_emergency_change",
            (&pool, &ParameterType::LTV, &5_000i128),
            || {
                client(&te).propose_emergency_change(&pool, &ParameterType::LTV, &5_000);
            },
        );
        te.env
            .ledger()
            .set_timestamp(EMERGENCY_TIMELOCK_SECONDS + 1);
        with_governance_auth(&te, "execute_emergency_override", (&1u64,), || {
            client(&te).execute_emergency_override(&1);
        });
        assert!(client(&te).is_emergency_active());

        with_governance_auth(&te, "clear_emergency_override", (), || {
            client(&te).clear_emergency_override();
        });
        assert!(!client(&te).is_emergency_active());
    }

    #[test]
    fn test_reject_proposal() {
        let te = setup();
        let pool = Address::generate(&te.env);
        with_governance_auth(&te, "register_pool", (&pool,), || {
            client(&te).register_pool(&pool);
        });
        with_governance_auth(
            &te,
            "propose_change",
            (
                &pool,
                &ParameterType::ReserveFactor,
                &1_000i128,
                &STANDARD_TIMELOCK_SECONDS,
            ),
            || {
                client(&te).propose_change(
                    &pool,
                    &ParameterType::ReserveFactor,
                    &1_000,
                    &STANDARD_TIMELOCK_SECONDS,
                );
            },
        );
        with_governance_auth(&te, "reject_proposal", (&1u64,), || {
            client(&te).reject_proposal(&1);
        });

        let proposal = client(&te).get_proposal(&1);
        assert!(proposal.rejected);
        assert!(!proposal.accepted);
    }

    #[test]
    fn test_parameter_type_validation() {
        assert!(ParameterType::LTV.validate_range(5_000));
        assert!(!ParameterType::LTV.validate_range(0));
        assert!(!ParameterType::LTV.validate_range(9_500));
        assert!(ParameterType::LiquidationIncentive.validate_range(1_000));
        assert!(!ParameterType::LiquidationIncentive.validate_range(500));
        assert!(!ParameterType::LiquidationIncentive.validate_range(2_500));
    }

    #[test]
    fn test_multiple_different_parameters() {
        let te = setup();
        let pool = Address::generate(&te.env);
        with_governance_auth(&te, "register_pool", (&pool,), || {
            client(&te).register_pool(&pool);
        });
        with_governance_auth(
            &te,
            "propose_change",
            (
                &pool,
                &ParameterType::LTV,
                &6_500i128,
                &RISK_TIMELOCK_SECONDS,
            ),
            || {
                client(&te).propose_change(
                    &pool,
                    &ParameterType::LTV,
                    &6_500,
                    &RISK_TIMELOCK_SECONDS,
                );
            },
        );
        with_governance_auth(
            &te,
            "propose_change",
            (
                &pool,
                &ParameterType::ReserveFactor,
                &500i128,
                &STANDARD_TIMELOCK_SECONDS,
            ),
            || {
                client(&te).propose_change(
                    &pool,
                    &ParameterType::ReserveFactor,
                    &500,
                    &STANDARD_TIMELOCK_SECONDS,
                );
            },
        );
    }
}
