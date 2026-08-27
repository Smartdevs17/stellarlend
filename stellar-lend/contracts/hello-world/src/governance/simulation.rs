use soroban_sdk::{Address, Env, String, Vec};

use crate::errors::GovernanceError;
use crate::storage::GovernanceDataKey;
use crate::types::{
    GovernanceAnalytics, GovernanceConfig, ParameterOptimizationRecommendation, Proposal,
    ProposalDryRunResult, ProposalSimulationResult, ProposalType, StateDiffEntry,
    BASIS_POINTS_SCALE,
};

use super::get_admin;

fn compute_simulation(
    env: &Env,
    proposal: &Proposal,
    config: &GovernanceConfig,
) -> ProposalSimulationResult {
    let now = env.ledger().timestamp();

    let total_votes = proposal.for_votes + proposal.against_votes + proposal.abstain_votes;
    let quorum_required = (total_votes * config.quorum_bps as i128) / BASIS_POINTS_SCALE;
    let quorum_reached = total_votes >= quorum_required;

    let threshold_votes =
        (proposal.total_voting_power * proposal.voting_threshold) / BASIS_POINTS_SCALE;
    let threshold_met = proposal.for_votes >= threshold_votes;

    let would_succeed = quorum_reached && threshold_met;
    let note = if would_succeed {
        String::from_str(env, "simulation: would succeed with current votes")
    } else {
        String::from_str(env, "simulation: would fail with current votes")
    };

    ProposalSimulationResult {
        proposal_id: proposal.id,
        now,
        would_succeed,
        quorum_required,
        quorum_reached,
        threshold_votes,
        threshold_met,
        for_votes: proposal.for_votes,
        against_votes: proposal.against_votes,
        abstain_votes: proposal.abstain_votes,
        total_voting_power: proposal.total_voting_power,
        note,
    }
}

pub fn simulate_proposal(
    env: &Env,
    proposal_id: u64,
) -> Result<ProposalSimulationResult, GovernanceError> {
    let config: GovernanceConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Config)
        .ok_or(GovernanceError::NotInitialized)?;

    let proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;

    let result = compute_simulation(env, &proposal, &config);
    env.storage().persistent().set(
        &GovernanceDataKey::ProposalSimulationCache(proposal_id),
        &result,
    );
    Ok(result)
}

pub fn get_simulation_cache(env: &Env, proposal_id: u64) -> Option<ProposalSimulationResult> {
    env.storage()
        .persistent()
        .get(&GovernanceDataKey::ProposalSimulationCache(proposal_id))
}

fn current_protocol_snapshot(env: &Env) -> (i128, i128, i128, i128, i128, i128, i128, bool) {
    let params = crate::risk_params::get_risk_params(env);
    let mcr = params
        .as_ref()
        .map(|p| p.min_collateral_ratio)
        .unwrap_or(11_000);
    let lt = params
        .as_ref()
        .map(|p| p.liquidation_threshold)
        .unwrap_or(10_500);
    let cf = params.as_ref().map(|p| p.close_factor).unwrap_or(5_000);
    let li = params
        .as_ref()
        .map(|p| p.liquidation_incentive)
        .unwrap_or(1_000);
    let tvl = crate::analytics::get_total_value_locked(env).unwrap_or(0);
    let borrow_apy = crate::interest_rate::get_current_borrow_rate(env).unwrap_or(0);
    let supply_apy = crate::interest_rate::get_current_supply_rate(env).unwrap_or(0);
    let paused = crate::risk_management::is_emergency_paused(env);
    (mcr, lt, cf, li, tvl, borrow_apy, supply_apy, paused)
}

fn estimate_execution_gas(proposal_type: &ProposalType) -> u64 {
    match proposal_type {
        ProposalType::MinCollateralRatio(_) => 45_000,
        ProposalType::RiskParams(_, _, _, _) => 72_000,
        ProposalType::InterestRateConfig(_) => 68_000,
        ProposalType::PauseSwitch(_, _) => 28_000,
        ProposalType::EmergencyPause(_) => 24_000,
        ProposalType::GenericAction(_) => 90_000,
    }
}

/// Dry-run proposal execution: state diff, TVL/APY/risk impact, gas estimate.
pub fn simulate_proposal_dry_run(
    env: &Env,
    proposal_id: u64,
) -> Result<ProposalDryRunResult, GovernanceError> {
    if let Some(cached) = get_dry_run_cache(env, proposal_id) {
        return Ok(cached);
    }

    let proposal: Proposal = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::Proposal(proposal_id))
        .ok_or(GovernanceError::ProposalNotFound)?;

    let vote_sim = {
        let config: GovernanceConfig = env
            .storage()
            .instance()
            .get(&GovernanceDataKey::Config)
            .ok_or(GovernanceError::NotInitialized)?;
        compute_simulation(env, &proposal, &config)
    };

    let (mcr, lt, cf, li, tvl, borrow_apy, _supply_apy, paused) = current_protocol_snapshot(env);

    let mut proposed_mcr = mcr;
    let mut proposed_lt = lt;
    let mut proposed_cf = cf;
    let mut proposed_li = li;
    let mut proposed_borrow_apy = borrow_apy;
    let mut proposed_paused = paused;

    match &proposal.proposal_type {
        ProposalType::MinCollateralRatio(ratio) => {
            proposed_mcr = *ratio;
        }
        ProposalType::RiskParams(new_mcr, new_lt, new_cf, new_li) => {
            if let Some(v) = new_mcr {
                proposed_mcr = *v;
            }
            if let Some(v) = new_lt {
                proposed_lt = *v;
            }
            if let Some(v) = new_cf {
                proposed_cf = *v;
            }
            if let Some(v) = new_li {
                proposed_li = *v;
            }
        }
        ProposalType::InterestRateConfig(params) => {
            if let Some(base) = params.base_rate_bps {
                proposed_borrow_apy = base;
            }
        }
        ProposalType::EmergencyPause(p) => {
            proposed_paused = *p;
        }
        ProposalType::PauseSwitch(_, p) => {
            proposed_paused = *p;
        }
        ProposalType::GenericAction(_) => {}
    }

    let mut tvl_delta: i128 = 0;
    if proposed_mcr != mcr && mcr > 0 {
        tvl_delta = -(tvl * (proposed_mcr - mcr)) / mcr / 10;
    }
    if proposed_paused && !paused {
        tvl_delta -= tvl / 20;
    }

    let apy_delta_bps = proposed_borrow_apy - borrow_apy;

    let risk_score_delta = (lt - proposed_lt) / 10 + (proposed_cf - cf) / 20 + (proposed_li - li) / 20;

    let mut diffs = Vec::new(env);
    if proposed_mcr != mcr {
        diffs.push_back(StateDiffEntry {
            field: String::from_str(env, "min_collateral_ratio"),
            current_value: mcr,
            proposed_value: proposed_mcr,
        });
    }
    if proposed_lt != lt {
        diffs.push_back(StateDiffEntry {
            field: String::from_str(env, "liquidation_threshold"),
            current_value: lt,
            proposed_value: proposed_lt,
        });
    }
    if proposed_cf != cf {
        diffs.push_back(StateDiffEntry {
            field: String::from_str(env, "close_factor"),
            current_value: cf,
            proposed_value: proposed_cf,
        });
    }
    if proposed_li != li {
        diffs.push_back(StateDiffEntry {
            field: String::from_str(env, "liquidation_incentive"),
            current_value: li,
            proposed_value: proposed_li,
        });
    }
    if proposed_borrow_apy != borrow_apy {
        diffs.push_back(StateDiffEntry {
            field: String::from_str(env, "borrow_apy_bps"),
            current_value: borrow_apy,
            proposed_value: proposed_borrow_apy,
        });
    }
    if proposed_paused != paused {
        diffs.push_back(StateDiffEntry {
            field: String::from_str(env, "emergency_pause"),
            current_value: if paused { 1 } else { 0 },
            proposed_value: if proposed_paused { 1 } else { 0 },
        });
    }

    let result = ProposalDryRunResult {
        proposal_id,
        would_succeed: vote_sim.would_succeed,
        tvl_delta,
        apy_delta_bps,
        risk_score_delta,
        gas_units_estimate: estimate_execution_gas(&proposal.proposal_type),
        diffs,
        simulated_at: env.ledger().timestamp(),
    };

    env.storage().persistent().set(
        &GovernanceDataKey::ProposalDryRunCache(proposal_id),
        &result,
    );
    Ok(result)
}

pub fn get_dry_run_cache(env: &Env, proposal_id: u64) -> Option<ProposalDryRunResult> {
    env.storage()
        .persistent()
        .get(&GovernanceDataKey::ProposalDryRunCache(proposal_id))
}

/// Get parameter optimization recommendation based on governance analytics.
pub fn get_parameter_optimization_recommendation(
    env: &Env,
) -> Result<ParameterOptimizationRecommendation, GovernanceError> {
    if let Some(cached) = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::ParameterOptimizationCache)
    {
        return Ok(cached);
    }

    let config: GovernanceConfig = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::Config)
        .ok_or(GovernanceError::NotInitialized)?;

    let analytics: GovernanceAnalytics = env
        .storage()
        .persistent()
        .get(&GovernanceDataKey::GovernanceAnalytics)
        .unwrap_or(GovernanceAnalytics {
            total_proposals: 0,
            total_votes: 0,
            suspicious_proposals: 0,
            last_suspicious_at: 0,
            max_single_voter_power: 0,
        });

    let votes_per_proposal = analytics
        .total_votes
        .checked_div(analytics.total_proposals)
        .unwrap_or(0);

    let mut suggested_quorum_bps = config.quorum_bps;
    if votes_per_proposal < 10 && suggested_quorum_bps > 2_000 {
        suggested_quorum_bps = suggested_quorum_bps.saturating_sub(500);
    }
    if analytics.suspicious_proposals > 0 {
        suggested_quorum_bps = suggested_quorum_bps.saturating_add(250).min(9_000);
    }

    let mut suggested_vote_threshold_bps = config.default_voting_threshold;
    if analytics.suspicious_proposals > 0 {
        suggested_vote_threshold_bps = (suggested_vote_threshold_bps + 250).min(BASIS_POINTS_SCALE);
    }

    let transparency_note = String::from_str(
        env,
        "recommendation derived from on-chain analytics; governance may override",
    );

    let recommendation = ParameterOptimizationRecommendation {
        generated_at: env.ledger().timestamp(),
        suggested_quorum_bps,
        suggested_vote_threshold_bps,
        suggested_voting_period: config.voting_period,
        transparency_note,
    };

    env.storage().persistent().set(
        &GovernanceDataKey::ParameterOptimizationCache,
        &recommendation,
    );

    Ok(recommendation)
}
