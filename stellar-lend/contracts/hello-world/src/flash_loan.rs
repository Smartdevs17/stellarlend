// SPDX-License-Identifier: Apache-2.0
use soroban_sdk::{contracterror, Address, Env, Vec as SorobanVec};
use stellarlend_flash_loan::{DefaultExecutor, FlashLoan, LoanPrimitive};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FlashLoanError {
    InvalidAmount = 1,
    InvalidAsset = 2,
    InsufficientLiquidity = 3,
    FlashLoanPaused = 4,
    NotRepaid = 5,
    InsufficientRepayment = 6,
    Overflow = 7,
    Reentrancy = 8,
    InvalidCallback = 9,
    CallbackFailed = 10,
    ExceedsLiquidityCap = 11,
    ExcessivePriceImpact = 12,
    ConcurrentLoan = 13,
    PriceManipulationDetected = 14,
    Expired = 15,
    Unprofitable = 16,
    EmptyLegs = 17,
    TooManyLegs = 18,
}

/// Wrapper for backward compatibility
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashLoanContract;

impl FlashLoanContract {
    pub fn execute_flash_loan(
        env: &Env,
        asset: Address,
        amount: i128,
        borrower: Address,
        executor: Address,
    ) -> SorobanVec<i128> {
        let primitive = LoanPrimitive::<()>::new(env, asset, amount, borrower);
        let primitives = [primitive];
        FlashLoan::execute(env, &DefaultExecutor, &primitives)
    }
}
