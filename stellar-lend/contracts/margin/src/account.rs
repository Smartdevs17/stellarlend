use soroban_sdk::{contracttype, Address, Vec};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
#[contracttype]
pub enum MarginMode {
    Isolated = 0,
    Cross = 1,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
#[contracttype]
pub enum MarginCallLevel {
    Safe = 0,
    Warning = 1,
    Liquidation = 2,
    ForcedClose = 3,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Position {
    pub asset: Address,
    pub amount: i128,
    pub debt: i128,
    pub entry_price: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct CollateralAssetConfig {
    pub asset: Address,
    /// Collateral factor (LTV) in basis points (e.g., 7500 = 75%)
    pub collateral_factor: i128,
    /// Liquidation threshold in basis points (e.g., 8000 = 80%)
    pub liquidation_threshold: i128,
    /// Asset price normalized to base decimals (e.g. 10_000_000 for $1.00 with 7 decimals)
    pub price: i128,
    /// Decimals for normalization (e.g. 7)
    pub decimals: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct CrossMarginSummary {
    pub total_collateral_value: i128,
    pub weighted_borrow_power: i128,
    pub liquidation_collateral_value: i128,
    pub total_debt_value: i128,
    pub health_factor_bps: i128,
    pub margin_call_level: MarginCallLevel,
    pub is_liquidatable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MarginAccount {
    pub owner: Address,
    pub mode: MarginMode,
    pub positions: Vec<Position>,
    pub total_collateral_value: i128,
    pub total_debt_value: i128,
}

impl MarginAccount {
    pub fn is_isolated(&self) -> bool {
        self.mode == MarginMode::Isolated
    }

    pub fn is_cross(&self) -> bool {
        self.mode == MarginMode::Cross
    }
}
