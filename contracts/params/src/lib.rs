#![no_std]
//! Typed parameter primitives for the lending pool (issue #1760).

pub mod registry;

pub use registry::{
    AssetParameters, AssetParametersUpdate, ParameterError, ParameterRegistry, BPS_DIVISOR,
    MAX_ASSETS, MAX_RESERVE_FACTOR_BPS,
};
