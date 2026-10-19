// SPDX-License-Identifier: Apache-2.0
use soroban_sdk::{contractimport, symbol, Env, Vec as SorobanVec, token, Address};

/// Core abstraction for composable loan operations
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoanPrimitive<T> {
    pub asset: token::Token,
    pub amount: i128,
    pub borrower: Address,
    pub _marker: std::marker::PhantomData<T>,
}

impl<T> LoanPrimitive<T> {
    pub fn new(env: &Env, asset: token::Token, amount: i128, borrower: Address) -> Self {
        Self {
            asset,
            amount,
            borrower,
            _marker: std::marker::PhantomData,
        }
    }
}

/// Flash loan contract wrapper
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashLoan;

impl FlashLoan {
    pub fn execute<
        E: FlashLoanExecutor,
    >(
        env: &Env,
        executor: &E,
        primitives: &[LoanPrimitive<E::Operation>],
    ) -> SorobanVec<i128> {
        let mut results = SorobanVec::new(env);
        for primitive in primitives {
            let result = executor.execute(env, primitive);
            results.push_back(env, result);
        }
        results
    }
}