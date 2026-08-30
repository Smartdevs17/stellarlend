use soroban_sdk::Env;

use crate::errors::GovernanceError;
use crate::types::ProposalType;

use super::get_admin;

/// Execute a proposal type — the single source of truth for proposal execution.
/// This function is shared by governance proposals, multisig execution, and
/// timelock operations, eliminating the previous duplication.
pub fn execute_proposal_type(env: &Env, proposal_type: &ProposalType) -> Result<(), GovernanceError> {
    match proposal_type {
        ProposalType::MinCollateralRatio(ratio) => {
            crate::risk_params::set_risk_params(env, Some(*ratio), None, None, None)
                .map_err(|_| GovernanceError::ExecutionFailed)?;
        }
        ProposalType::RiskParams(mcr, lt, cf, li) => {
            crate::risk_params::set_risk_params(env, *mcr, *lt, *cf, *li)
                .map_err(|_| GovernanceError::ExecutionFailed)?;
        }
        ProposalType::InterestRateConfig(params) => {
            let admin = get_admin(env).ok_or(GovernanceError::NotInitialized)?;
            crate::interest_rate::update_interest_rate_config(
                env,
                admin,
                params.base_rate_bps,
                params.kink_utilization_bps,
                params.multiplier_bps,
                params.jump_multiplier_bps,
                params.rate_floor_bps,
                params.rate_ceiling_bps,
                params.spread_bps,
            )
            .map_err(|_| GovernanceError::ExecutionFailed)?;
        }
        ProposalType::PauseSwitch(op, paused) => {
            let admin = get_admin(env).ok_or(GovernanceError::NotInitialized)?;
            crate::risk_management::set_pause_switch(env, admin, op.clone(), *paused)
                .map_err(|_| GovernanceError::ExecutionFailed)?;
        }
        ProposalType::EmergencyPause(paused) => {
            let admin = get_admin(env).ok_or(GovernanceError::NotInitialized)?;
            crate::risk_management::set_emergency_pause(env, admin, *paused)
                .map_err(|_| GovernanceError::ExecutionFailed)?;
        }
        ProposalType::GenericAction(_) => {
            return Err(GovernanceError::InvalidProposalType);
        }
    }
    Ok(())
}
