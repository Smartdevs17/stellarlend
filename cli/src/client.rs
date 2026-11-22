use crate::error::CliError;
use crate::format::format_amount;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxResult {
    pub tx_hash: String,
    pub status: String,
    pub contract_id: String,
    pub method: String,
    pub value: Option<serde_json::Value>,
}

impl fmt::Display for TxResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Transaction Confirmed:")?;
        writeln!(f, "  Tx Hash:     {}", self.tx_hash)?;
        writeln!(f, "  Status:      {}", self.status)?;
        writeln!(f, "  Contract ID: {}", self.contract_id)?;
        writeln!(f, "  Method:      {}", self.method)?;
        if let Some(ref val) = self.value {
            writeln!(f, "  Result:      {}", val)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionResult {
    pub account: String,
    pub collateral: i128,
    pub debt: i128,
    pub health_factor: f64,
    pub liquidatable: bool,
}

impl fmt::Display for PositionResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "User Position:")?;
        writeln!(f, "  Account:       {}", self.account)?;
        writeln!(f, "  Collateral:    {} XLM", format_amount(self.collateral))?;
        writeln!(f, "  Debt:          {} XLM", format_amount(self.debt))?;
        writeln!(f, "  Health Factor: {:.4}", self.health_factor)?;
        writeln!(
            f,
            "  Liquidatable:  {}",
            if self.liquidatable { "YES" } else { "No" }
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolResult {
    pub total_collateral: i128,
    pub total_debt: i128,
    pub utilization_rate: f64,
    pub supply_apy: f64,
    pub borrow_apy: f64,
}

impl fmt::Display for PoolResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Pool Statistics:")?;
        writeln!(
            f,
            "  Total Collateral: {} XLM",
            format_amount(self.total_collateral)
        )?;
        writeln!(
            f,
            "  Total Debt:       {} XLM",
            format_amount(self.total_debt)
        )?;
        writeln!(
            f,
            "  Utilization:      {:.2}%",
            self.utilization_rate * 100.0
        )?;
        writeln!(f, "  Supply APY:       {:.2}%", self.supply_apy * 100.0)?;
        writeln!(f, "  Borrow APY:       {:.2}%", self.borrow_apy * 100.0)
    }
}

/// Simulated client facade for StellarLend Soroban interactions.
pub struct StellarLendClient {
    pub rpc_url: String,
    pub contract_id: String,
}

impl StellarLendClient {
    pub fn new(rpc_url: String, contract_id: String) -> Self {
        Self {
            rpc_url,
            contract_id,
        }
    }

    pub fn deposit(
        &self,
        account: &str,
        amount: i128,
        asset: Option<&str>,
    ) -> Result<TxResult, CliError> {
        tracing::info!(account = %account, amount = %amount, asset = ?asset, "Executing deposit");
        let hash = format!("tx_dep_{:x}_{:x}", account.len(), amount);
        Ok(TxResult {
            tx_hash: hash,
            status: "SUCCESS".to_string(),
            contract_id: self.contract_id.clone(),
            method: if asset.is_some() {
                "deposit_collateral_asset".to_string()
            } else {
                "deposit_collateral".to_string()
            },
            value: Some(serde_json::json!({
                "deposited_stroops": amount,
                "asset": asset.unwrap_or("native"),
            })),
        })
    }

    pub fn withdraw(
        &self,
        account: &str,
        amount: i128,
        asset: Option<&str>,
    ) -> Result<TxResult, CliError> {
        tracing::info!(account = %account, amount = %amount, asset = ?asset, "Executing withdraw");
        let hash = format!("tx_wdr_{:x}_{:x}", account.len(), amount);
        Ok(TxResult {
            tx_hash: hash,
            status: "SUCCESS".to_string(),
            contract_id: self.contract_id.clone(),
            method: if asset.is_some() {
                "withdraw_asset".to_string()
            } else {
                "withdraw".to_string()
            },
            value: Some(serde_json::json!({
                "withdrawn_stroops": amount,
                "asset": asset.unwrap_or("native"),
            })),
        })
    }

    pub fn borrow(
        &self,
        account: &str,
        amount: i128,
        asset: Option<&str>,
    ) -> Result<TxResult, CliError> {
        tracing::info!(account = %account, amount = %amount, asset = ?asset, "Executing borrow");
        let hash = format!("tx_brw_{:x}_{:x}", account.len(), amount);
        Ok(TxResult {
            tx_hash: hash,
            status: "SUCCESS".to_string(),
            contract_id: self.contract_id.clone(),
            method: if asset.is_some() {
                "borrow_asset".to_string()
            } else {
                "borrow".to_string()
            },
            value: Some(serde_json::json!({
                "borrowed_stroops": amount,
                "asset": asset.unwrap_or("native"),
            })),
        })
    }

    pub fn repay(
        &self,
        account: &str,
        amount: i128,
        asset: Option<&str>,
    ) -> Result<TxResult, CliError> {
        tracing::info!(account = %account, amount = %amount, asset = ?asset, "Executing repay");
        let hash = format!("tx_rpy_{:x}_{:x}", account.len(), amount);
        Ok(TxResult {
            tx_hash: hash,
            status: "SUCCESS".to_string(),
            contract_id: self.contract_id.clone(),
            method: if asset.is_some() {
                "repay_asset".to_string()
            } else {
                "repay".to_string()
            },
            value: Some(serde_json::json!({
                "repaid_stroops": if amount == i128::MAX { "max".to_string() } else { amount.to_string() },
                "asset": asset.unwrap_or("native"),
            })),
        })
    }

    pub fn liquidate(
        &self,
        liquidator: &str,
        borrower: &str,
        repay_amount: i128,
        collateral_asset: Option<&str>,
        debt_asset: Option<&str>,
    ) -> Result<TxResult, CliError> {
        tracing::info!(
            liquidator = %liquidator,
            borrower = %borrower,
            repay_amount = %repay_amount,
            "Executing liquidation"
        );
        let hash = format!("tx_liq_{:x}_{:x}", borrower.len(), repay_amount);
        Ok(TxResult {
            tx_hash: hash,
            status: "SUCCESS".to_string(),
            contract_id: self.contract_id.clone(),
            method: "liquidate".to_string(),
            value: Some(serde_json::json!({
                "liquidator": liquidator,
                "borrower": borrower,
                "repay_amount": repay_amount,
                "collateral_asset": collateral_asset,
                "debt_asset": debt_asset,
            })),
        })
    }

    pub fn get_position(&self, account: &str) -> Result<PositionResult, CliError> {
        tracing::info!(account = %account, "Querying position");
        Ok(PositionResult {
            account: account.to_string(),
            collateral: 5_000_000_000,
            debt: 2_000_000_000,
            health_factor: 1.85,
            liquidatable: false,
        })
    }

    pub fn get_pool(&self, _asset: Option<&str>) -> Result<PoolResult, CliError> {
        tracing::info!("Querying pool statistics");
        Ok(PoolResult {
            total_collateral: 100_000_000_000_000,
            total_debt: 65_000_000_000_000,
            utilization_rate: 0.65,
            supply_apy: 0.045,
            borrow_apy: 0.078,
        })
    }
}
