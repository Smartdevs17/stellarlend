#![no_std]
#![allow(deprecated)]
pub mod cache;
pub mod events;
pub mod message_bus;
pub mod shared_types;
pub mod storage;
pub mod upgrade;

/// Re-exports of the unified error handling framework so contract crates that
/// already depend on `stellarlend-common` don't have to add
/// `stellarlend-errors` as a separate dependency just to normalize errors.
pub mod errors {
    pub use stellarlend_errors::{
        assert_error_code, lending_code_to_core, lending_code_to_core_or_internal, log_error,
        log_error_with_tag, recover, CoreError, ErrorAnalytics,
        IntoError, LendingCode, RecoveryDecision,
    };
    pub use stellarlend_errors::mapping as mapping;

    pub mod recovery {
        pub use stellarlend_errors::recovery::{hint, recover, RecoveryDecision};
    }
    pub mod logging {
        pub use stellarlend_errors::logging::{log_error, log_error_with_tag};
    }
    pub mod testing {
        pub use stellarlend_errors::testing::assert_code;
    }
}

#[cfg(test)]
mod protocol_integration_test;
