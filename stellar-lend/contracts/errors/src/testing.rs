//! Error testing utilities.
//!
//! Small helpers that make it ergonomic to assert on normalized error categories in
//! `#[cfg(test)]` code across contract crates, reducing duplication of the
//! `assert_eq!(err as u32, ...)` boilerplate.

use crate::{decode_global_code, ContractError, CoreError};

/// Asserts that the numeric `code` corresponds to a specific [`CoreError`] category.
///
/// # Panics
/// Panics with a descriptive message when `code` does not map onto `expected`.
pub fn assert_code(code: u32, expected: CoreError) {
    assert_eq!(code, expected as u32, "error code mismatch");
}

/// Checks that a registered [`ContractError`] enum is internally consistent:
/// every variant resolves by its code, global codes decode back to the enum's
/// domain, and every message and suggestion is non-empty.
///
/// Call it from each contract crate's tests:
/// `stellarlend_errors::testing::assert_registry::<MyError>();`
///
/// # Panics
/// Panics describing the first inconsistency found.
pub fn assert_registry<E: ContractError + core::fmt::Debug + PartialEq>() {
    let variants = E::variants();
    assert!(!variants.is_empty(), "error registry is empty");
    for v in variants {
        let d = v.describe();
        assert_eq!(E::from_local_code(d.local_code), Some(*v), "{:?} does not round-trip", v);
        let (domain, local) =
            decode_global_code(d.global_code).expect("global code has unknown contract");
        assert_eq!(domain, E::DOMAIN, "{:?} global code decodes to wrong domain", v);
        assert_eq!(local, d.local_code, "{:?} global code decodes to wrong local code", v);
        assert!(!d.name.is_empty() && !d.message.is_empty(), "{:?} has no message", v);
        assert!(!d.suggestion().is_empty(), "{:?} has no suggestion", v);
    }
}

/// Convenience macro mirroring `assert_code` but usable inline.
#[macro_export]
macro_rules! assert_error_code {
    ($code:expr, $expected:ident) => {
        assert!(
            $code == $crate::CoreError::$expected as u32,
            "error code mismatch: got {} want {}",
            $code,
            stringify!($expected)
        );
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_code_matches() {
        assert_code(6, CoreError::Overflow);
    }

    #[test]
    #[should_panic]
    fn assert_code_mismatch_panics() {
        assert_code(1, CoreError::Overflow);
    }

    #[test]
    fn macro_assert_compiles_and_passes() {
        assert_error_code!(9, NotInitialized);
    }
}
