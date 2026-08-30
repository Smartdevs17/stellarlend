//! Error testing utilities.
//!
//! Small helpers that make it ergonomic to assert on normalized error categories in
//! `#[cfg(test)]` code across contract crates, reducing duplication of the
//! `assert_eq!(err as u32, ...)` boilerplate.

use crate::CoreError;

/// Asserts that the numeric `code` corresponds to a specific [`CoreError`] category.
///
/// # Panics
/// Panics with a descriptive message when `code` does not map onto `expected`.
pub fn assert_code(code: u32, expected: CoreError) {
    assert_eq!(code, expected as u32, "error code mismatch");
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
