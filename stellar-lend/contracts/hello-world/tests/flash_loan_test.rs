// SPDX-License-Identifier: Apache-2.0
use soroban_sdk::{testutils::*, Env, token, Address};
use stellar_lend_hello_world::flash_loan::FlashLoanContract;
use flash_loan::{FlashLoan, LoanPrimitive, FlashLoanExecutor};

struct TestExecutor;

impl FlashLoanExecutor for TestExecutor {
    type Operation = ();
    
    fn execute(&self, _env: &Env, primitive: &LoanPrimitive<()>) -> i128 {
        assert_eq!(primitive.amount, 100);
        42
    }
}

#[test]
fn test_flash_loan_execution() {
    let env = Env::default();
    let client = Client::new(env.clone());
    
    let asset = token::Token::new(
        env.clone(),
        Address::generate(&env),
        symbol::Symbol::new(&env, "XLM"),
    );
    
    let result = FlashLoan::execute(&env, &TestExecutor, &[LoanPrimitive::new(&env, asset, 100, Address::generate(&env))]);
    assert_eq!(result.get(&env)[0], 42);
}

#[test]
fn test_backward_compatibility() {
    let env = Env::default();
    let client = Client::new(env.clone());
    
    let asset = token::Token::new(
        env.clone(),
        Address::generate(&env),
        symbol::Symbol::new(&env, "XLM"),
    );
    
    let result = FlashLoanContract::execute_flash_loan(
        &env,
        asset,
        100,
        Address::generate(&env),
        Address::generate(&env),
    );
    assert_eq!(result.get(&env)[0], 0); // Default executor
}