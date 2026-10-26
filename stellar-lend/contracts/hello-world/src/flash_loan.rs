// SPDX-License-Identifier: Apache-2.0
use soroban_sdk::{Address, Env, Vec as SorobanVec};
use flash_loan::{DefaultExecutor, FlashLoan, FlashLoanExecutor, LoanPrimitive};

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
        let primitive = LoanPrimitive::new(env, asset, amount, borrower);
        let primitives = SorobanVec::from_slice(env, &[primitive]);
        FlashLoan::execute(env, &DefaultExecutor, &primitives)
    }
}