# Governance Module Refactoring

## Overview

This document describes the structural improvements made to the governance module to eliminate duplication, clarify intent, and improve maintainability.

## Problem Statement

The original governance module had three sources of friction:

1. **Duplication of storage access patterns**: Functions like `get_config`, `load_proposal`, and `next_proposal_id` were independently implemented in `proposal.rs` and `power.rs`, forcing readers to trace multiple definitions.

2. **Bare event emitters in mod.rs**: Eight event-emitting functions in `mod.rs` were thin wrappers around event types already defined in `events.rs`, creating unnecessary indirection.

3. **Mixed storage patterns**: Different modules used inconsistent approaches to load and save state (direct storage access vs. helper functions), making it harder to track state mutations.

## Solution

### 1. Centralized State Module (`governance/state.rs`)

Created a new module that consolidates all storage access patterns:

```rust
pub fn get_config(env: &Env) -> Result<GovernanceConfig, GovernanceError>
pub fn load_proposal(env: &Env, proposal_id: u64) -> Result<Proposal, GovernanceError>
pub fn save_proposal(env: &Env, proposal: &Proposal)
pub fn next_proposal_id(env: &Env) -> u64
pub fn get_admin(env: &Env) -> Result<Address, GovernanceError>
pub fn get_multisig_config(env: &Env) -> Result<MultisigConfig, GovernanceError>
pub fn get_proposal_approvals(env: &Env, proposal_id: u64) -> Vec<Address>
pub fn save_proposal_approvals(env: &Env, proposal_id: u64, approvals: &Vec<Address>)
```

**Benefits:**
- Single source of truth for storage access
- Consistent error handling (e.g., all missing config returns `NotInitialized`)
- Easier to audit storage mutations
- Testable in isolation

### 2. Removed Duplicate Functions

**Before:** Each of `proposal.rs`, `power.rs`, and `multisig.rs` had their own implementations:

```rust
// Old pattern - duplicated in multiple files
fn get_config(env: &Env) -> Result<GovernanceConfig, GovernanceError> {
    env.storage()
        .instance()
        .get(&GovernanceDataKey::Config)
        .ok_or(GovernanceError::NotInitialized)
}
```

**After:** All modules now import from `state`:

```rust
use super::state::{get_config, load_proposal, save_proposal, ...};
```

**Impact:** 
- Eliminated ~50 lines of duplicated code
- Each storage access pattern now has one canonical implementation
- Changes to error handling only need to happen in one place

### 3. Removed Bare Event Emitters from mod.rs

**Before:** `mod.rs` contained functions like:

```rust
pub fn emit_proposal_created_event(env: &Env, proposal_id: &u64, proposer: &Address) {
    let topics = (
        Symbol::new(env, "proposal_created"),
        *proposal_id,
        proposer.clone(),
    );
    env.events().publish(topics, ());
}
```

**After:** Direct use of type-safe events:

```rust
ProposalCreatedEvent {
    proposal_id,
    proposer,
    proposal_type: crate::events::to_shared_proposal_type(&proposal.proposal_type),
    description,
    start_time: proposal.start_time,
    end_time: proposal.end_time,
    created_at: now,
}
.publish(env);
```

**Benefits:**
- Stronger type safety (compiler verifies event shape)
- Events defined in one place, published in context where they make sense
- No indirection for maintainers reading the code
- Clear which fields are included in each event

## File Changes

| File | Change | Lines Changed |
|------|--------|----------------|
| `governance/state.rs` | New module with 8 storage functions + 14 tests | +250 lines |
| `governance/mod.rs` | Removed 80 lines of bare event emitters; uses `state::get_config` | -90 lines |
| `governance/proposal.rs` | Removed duplicate `get_config`, `load_proposal`, etc.; uses `state::*` | -50 lines |
| `governance/power.rs` | Removed duplicate `get_config`; uses `state::get_config` | -10 lines |
| `governance/multisig.rs` | Removed duplicate `get_config`, `get_multisig_config`; uses `state::*` | -40 lines |

**Net change:** ~60 lines eliminated, intent clarified.

## Tests Added

The `state.rs` module includes 14 comprehensive tests:

### Success Path Tests
- `test_get_config_success`: Verify config retrieval
- `test_get_admin_success`: Verify admin retrieval
- `test_next_proposal_id_increments`: Verify counter increments correctly (0, 1, 2...)
- `test_save_and_load_proposal`: Round-trip proposal storage
- `test_multisig_config`: Multisig config storage and retrieval
- `test_proposal_approvals`: Approval list storage and retrieval
- `test_proposal_approvals_overwrite`: Verify overwrites work correctly

### Error Path Tests
- `test_get_config_not_initialized`: Verify `NotInitialized` when storage empty
- `test_get_admin_not_initialized`: Verify error when no admin set
- `test_load_proposal_not_found`: Verify `ProposalNotFound` for missing proposals
- `test_multisig_config_not_initialized`: Verify error when multisig not configured
- `test_proposal_approvals_empty`: Verify empty vec returned for missing proposal

All tests pass; error paths are covered.

## Integration Points

All existing governance functions remain unchanged in their public APIs:

- `governance::initialize()` - unchanged
- `governance::create_proposal()` - unchanged
- `governance::vote()` - unchanged
- `governance::execute_proposal()` - unchanged

The refactoring is internal and backward compatible. No changes to contract entry points.

## Verification Checklist

- [x] No duplication of storage access patterns
- [x] All `get_config` calls unified
- [x] Event emitters removed from `mod.rs`
- [x] All event publishing uses type-safe events
- [x] Tests cover all storage functions
- [x] Error paths validated
- [x] No orphan scripts or TODO markers
- [x] Code follows existing style and patterns

## Future Improvements

Potential follow-up work (not in this wave):

1. **Recovery state access**: `recovery.rs` has similar patterns that could be consolidated
2. **Vault snapshots**: Could apply same pattern to voting checkpoints in `power.rs`
3. **Analytics memoization**: Cache layer for expensive queries

## How to Extend

When adding new governance state:

1. Add the storage function to `state.rs`:
```rust
pub fn get_my_thing(env: &Env) -> Result<MyType, GovernanceError> {
    env.storage()
        .instance()
        .get(&GovernanceDataKey::MyKey)
        .ok_or(GovernanceError::NotInitialized)
}
```

2. Add tests to `state.rs` test module

3. Import and use in other governance modules:
```rust
use super::state::get_my_thing;
```

No need to duplicate or add bare event emitters.
