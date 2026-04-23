#![no_std]
#![allow(deprecated)]
mod bridge;

pub use bridge::{BridgeContract, ContractError};

#[cfg(any(test, feature = "testutils"))]
pub use bridge::BridgeContractClient;

#[cfg(test)]
mod math_safety_test;
#[cfg(test)]
mod test;
