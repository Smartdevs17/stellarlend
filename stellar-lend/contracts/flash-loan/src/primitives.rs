// SPDX-License-Identifier: Apache-2.0
use soroban_sdk::{Address, Env, Vec as SorobanVec};

use crate::executor::FlashLoanExecutor;

/// Core abstraction for composable loan operations
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoanPrimitive<T> {
    pub asset: Address,
    pub amount: i128,
    pub borrower: Address,
    pub _marker: std::marker::PhantomData<T>,
}

impl<T> LoanPrimitive<T> {
    pub fn new(_env: &Env, asset: Address, amount: i128, borrower: Address) -> Self {
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
    pub fn execute<E: FlashLoanExecutor>(
        env: &Env,
        executor: &E,
        primitives: &[LoanPrimitive<E::Operation>],
    ) -> SorobanVec<i128> {
        let mut results = SorobanVec::new(env);
        for primitive in primitives {
            let result = executor.execute(env, primitive);
            results.push_back(result);
        }
        results
    }
}
