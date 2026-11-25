#![no_std]
use soroban_sdk::{contract, contractimpl, contracterror, Address, Env, BytesN, Symbol, symbol_short, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FlashLoanError {
    NotAuthorized = 1,
    CallbackNotSafelisted = 2,
    ExecutionFailed = 3,
    InsufficientRepayment = 4,
    InvalidFee = 5,
}

const ADMIN: Symbol = symbol_short!("ADMIN");
const SAFELIST: Symbol = symbol_short!("SAFELIST");

#[contract]
pub struct FlashLoanHook;

#[contractimpl]
impl FlashLoanHook {
    pub fn init(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&ADMIN, &admin);
    }

    pub fn set_safelist(env: Env, caller: Address, target_contract: Address, allowed: bool) -> Result<(), FlashLoanError> {
        caller.require_auth();
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        if caller != admin {
            return Err(FlashLoanError::NotAuthorized);
        }

        let key = (SAFELIST, target_contract);
        env.storage().instance().set(&key, &allowed);
        Ok(())
    }

    pub fn is_safelisted(env: Env, target_contract: Address) -> bool {
        let key = (SAFELIST, target_contract);
        env.storage().instance().get(&key).unwrap_or(false)
    }

    pub fn execute_flashloan_hook(
        env: Env,
        caller: Address,
        receiver_contract: Address,
        token: Address,
        amount: i128,
        fee: i128
    ) -> Result<bool, FlashLoanError> {
        caller.require_auth();

        if !Self::is_safelisted(env.clone(), receiver_contract.clone()) {
            return Err(FlashLoanError::CallbackNotSafelisted);
        }

        if amount <= 0 || fee < 0 {
            return Err(FlashLoanError::InsufficientRepayment);
        }

        Ok(true)
    }
}
