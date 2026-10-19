// Lending uses the shared upgrade manager from `stellarlend-common`.
//
// Keeping this module as a thin re-export avoids code duplication while allowing
// existing tests and downstream tooling to refer to `crate::upgrade::*`.

pub use stellarlend_common::upgrade::{
    UpgradeError,
    UpgradeManager,
    UpgradeManagerClient,
    UpgradeStage,
    UpgradeStatus,
    // Re-exported so `upgrade_test.rs` can reference the timelock windows it
    // asserts against. These were missing from this list, which left
    // `crate::upgrade::STANDARD_TIMELOCK_SECS` unresolvable and stopped the
    // whole lending test target from compiling.
    EMERGENCY_TIMELOCK_SECS,
    STANDARD_TIMELOCK_SECS,
};
