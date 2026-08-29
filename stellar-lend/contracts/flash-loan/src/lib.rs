#![no_std]

use soroban_sdk::{contractclient, contracttype, Address, Env};

/// Standard interface for contracts that want to receive flash loans
#[contractclient(name = "FlashLoanReceiverClient")]
pub trait FlashLoanReceiver {
    /// Callback executed by the flash loan module after funds are transferred.
    /// The receiver must authorize the transfer of `amount + fee` back to the pool
    /// before this function returns.
    fn on_flash_loan(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
        fee: i128,
    );
}

/// Metrics tracked for flash loan operations
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FlashLoanMetrics {
    pub total_flash_loans: u64,
    pub total_volume: i128,
    pub total_fees_collected: i128,
}
