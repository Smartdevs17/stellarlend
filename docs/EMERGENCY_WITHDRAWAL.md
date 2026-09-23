# Lending Pool Emergency Withdrawal Mechanism

## Overview

The **Lending Pool Emergency Withdrawal Mechanism** guarantees lenders and liquidity providers immediate, unhindered access to their deposited assets during critical protocol disruptions, market shocks, oracle dysfunctions, or administrative pause events. It incorporates reduced emergency fee schedules, on-chain circuit breakers, transaction rate limits, end-to-end event logging, and comprehensive incident reporting.

---

## Key Features

### 1. Dedicated Emergency Withdrawal Function
- **Contract-Level Priority**: The Soroban smart contract implements `emergency_withdraw(env, user, asset, amount)`, which bypasses standard operational pauses (`PauseType::Withdraw` or `EmergencyState::Shutdown`).
- **Authorization**: Caller authentication is strictly required via `user.require_auth()`.
- **Atomic Position Settlement**: The user's deposit position and the protocol's global deposit aggregate are decremented atomically, eliminating reentrancy and phantom liquidity.

### 2. Reduced Emergency Fees
- **Fee Relief**: Under emergency circumstances, normal protocol withdrawal/liquidation penalties (standard 50 bps / 0.50%) are reduced to an emergency rate of **10 bps (0.10%)**, representing an **80% fee discount**.
- **Transparent Accounting**: Fees are computed and deducted on-chain, and net amounts are paid directly to the user's Stellar account.
- **Fee Savings Tracking**: All fee savings delivered to distressed lenders are tracked protocol-wide.

### 3. Emergency Limits & Safeguards
- **Per-Transaction Limit**: Configurable maximum cap per single emergency withdrawal (default: 500,000 units) to prevent instantaneous single-tx pool drainage.
- **Daily Per-User Limit**: Enforces a 24-hour cumulative cap per address (default: 1,000,000 units) to ensure equitable exit liquidity across all lenders.
- **Protocol Circuit Breakers**: Total pool drain velocity is monitored to preserve essential solvency buffers.

### 4. Emergency Event Tracking
- Emits structured on-chain events:
  ```rust
  pub struct EmergencyWithdrawEvent {
      pub user: Address,
      pub asset: Address,
      pub requested_amount: i128,
      pub fee_amount: i128,
      pub net_amount: i128,
      pub remaining_balance: i128,
      pub timestamp: u64,
  }
  ```
- Full off-chain event log tracking timestamps, tx hashes, affected positions, fee savings, and administrative interventions.

### 5. Emergency Reporting & Incident Audits
- Automated generator producing structured post-incident and real-time reports:
  - Protocol operational state and pause origin.
  - Cumulative emergency volume and fee discounts applied.
  - Distribution of withdrawals across supported collateral assets.
  - Chronological audit log of all emergency events.

### 6. Emergency Analytics
- Key metrics exposed via API and UI:
  - `totalEmergencyWithdrawn`: Cumulative capital recovered.
  - `totalFeeSavingsDelivered`: Net savings for lenders.
  - `hourlyDrainRate`: Liquidity velocity across active pools.
  - `uniqueUsersAffected`: Number of lenders successfully exiting positions.

---

## Contract Interface (Soroban)

```rust
// LendingContract methods
pub fn emergency_withdraw(
    env: Env,
    user: Address,
    asset: Address,
    amount: i128,
) -> Result<i128, WithdrawError>;

pub fn set_emergency_withdraw_limit(
    env: Env,
    max_amount: i128,
) -> Result<(), WithdrawError>;

pub fn get_emergency_stats(env: Env) -> (i128, i128);
```

---

## API Reference

### Execute Emergency Withdrawal
```http
POST /api/emergency/withdraw
Content-Type: application/json

{
  "userAddress": "G...",
  "assetAddress": "USDC",
  "amount": 10000
}
```

### Preview Reduced Fees
```http
GET /api/emergency/fee-preview?amount=10000
```

### Get Withdrawal Limits
```http
GET /api/emergency/limits
```

### Update Limits (Admin / Governance)
```http
PUT /api/emergency/limits
Content-Type: application/json

{
  "maxPerTransaction": 750000,
  "maxDailyPerUser": 1500000
}
```

### Generate Emergency Report
```http
GET /api/emergency/report
```

### Get Emergency Analytics
```http
GET /api/emergency/analytics
```
