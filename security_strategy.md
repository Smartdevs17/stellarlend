StellarLend Protection Strategy: Reentrancy & Security Documentation
To satisfy the security requirements for the stellarlend protocol, we are implementing a multi-layered defense strategy. This documentation outlines the what, why, and how of our approach to protecting user funds and contract integrity.

1. Core Protection Strategy
The protection strategy is built on three pillars:

Mutual Exclusion (The Guard): A state-based lock that prevents a contract from being re-entered before the first execution is complete.

Check-Effects-Interactions (CEI): A coding pattern that ensures all internal accounting is finalized before any external asset transfers occur.

Transient Storage: Utilizing Soroban’s temporary storage to manage locks efficiently without bloating the permanent ledger state.

2. Implementation Methodology
A. The Reentrancy Guard (The "Mutex")
We implement a ReentrancyGuard struct in a dedicated utility module. This guard uses the "Resource Acquisition Is Initialization" (RAII) pattern.

Initialization: When created, the guard checks for a specific key in temporary storage. If it exists, the contract panics (detecting reentrancy). If not, it sets the key.

Automatic Release: Because the guard implements the Drop trait, the lock is automatically removed when the function execution ends, even if the function returns early or fails.

B. Function Anatomy
Every external, state-changing entry point (e.g., deposit, withdraw, borrow, liquidate, flash_loan) will follow this strict structure:

Rust
pub fn entry_point(env: Env, ...) {
    // 1. Reentrancy Guard (Lock the contract)
    let _guard = ReentrancyGuard::new(&env);

    // 2. Checks (Auth, validation, input sanitization)
    user.require_auth();

    // 3. Effects (Update internal debt/collateral ledgers)
    update_internal_state(&env, ...);

    // 4. Interactions (External token transfers or callbacks)
    perform_external_transfer(&env, ...);
}
3. Technical Specifications
Guard Lifecycle Table
Phase	Action	Storage Impact
Call Start	ReentrancyGuard::new()	Sets Temporary key REENT_G
Execution	Contract Logic	REENT_G remains active; any nested call fails
Call End	Drop trait triggered	REENT_G is removed
Cross-Contract Security
By using a consistent key across all functions within a contract instance, we prevent Cross-Function Reentrancy. For example, an attacker cannot call withdraw and, while that is processing, call borrow to exploit a temporarily inflated balance.

4. Verification and Testing Plan
To ensure the protection is effective, the following tests will be implemented:

Positive Test: Verify that standard, honest transactions (Deposit -> Withdraw) function correctly without triggering the guard.

Negative (Reentrancy) Test: Create a "Malicious Mock Token." When the protocol calls transfer() on this token, the token will attempt to call back into the protocol's withdraw() function. The test must confirm that this attempt results in a contract panic.

Audit Trail: All state changes must be accompanied by events. This allows off-chain monitoring to verify that Interactions (transfers) always follow Effects (state updates).

5. Security Limitations & Considerations
Gas Management: The guard uses a small amount of gas for storage operations. This is a necessary trade-off for security.

Third-Party Contracts: While we protect our entry points, interactions with unknown third-party contracts (in flash loans, for instance) still require strict adherence to the CEI pattern, as the guard only protects our contract state.