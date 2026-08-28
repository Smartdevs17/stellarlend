# Lending Pool Fee Tier System with Loyalty Discounts

## Overview

The Fee Tier and Loyalty Discount System replaces flat fee models with dynamic, volume- and loyalty-adjusted discount tiers. Long-term protocol participants earn escalating fee discounts based on total deposits, borrow volume, account age, and withdrawal-free loyalty streaks.

---

## Fee Tiers Specification

| Tier Name | Base Discount | Loyalty Bonus | Min Deposits | Min Borrow Volume | Min Account Days | Min Loyal Days |
|-----------|---------------|---------------|--------------|-------------------|------------------|----------------|
| **Base** | 0% | 0% | $0 | $0 | 0 | 0 |
| **Bronze** | 5% (500 bps) | +0.5% (50 bps) | $2,500 | $1,000 | 14 days | 7 days |
| **Silver** | 10% (1,000 bps) | +1.0% (100 bps) | $10,000 | $5,000 | 30 days | 14 days |
| **Gold** | 25% (2,500 bps) | +2.5% (250 bps) | $50,000 | $25,000 | 90 days | 30 days |
| **Platinum** | 40% (4,000 bps) | +5.0% (500 bps) | $250,000 | $100,000 | 180 days | 90 days |
| **VIP Diamond** | 50% (5,000 bps) | +5.0% (500 bps) | $1,000,000 | $500,000 | 365 days | 180 days |

---

## Features

### 1. Dynamic Tier Qualification
- Evaluates four user dimensions:
  1. `totalDeposits`: Active collateral and lending balances.
  2. `borrowingVolume`: Cumulative borrowed capital.
  3. `accountAgeDays`: Duration since account creation on StellarLend.
  4. `daysSinceWithdrawal`: Loyalty streak rewarding capital stickiness.

### 2. Loyalty Discount Mechanism
- Users who refrain from withdrawing capital maintain an active loyalty streak (`daysSinceWithdrawal`).
- When `daysSinceWithdrawal >= minLoyalDays`, an additional loyalty bonus (up to 500 bps) stacks on top of the base tier discount.

### 3. Fee Transparency
- Lenders and borrowers receive itemized breakdowns for every fee assessed:
  - Nominal base fee (un-discounted)
  - Tier qualification & base tier discount amount
  - Loyalty bonus discount amount
  - Net fee payable
  - Comparative savings versus adjacent tiers

### 4. Protocol Fee Analytics
- Global tracking of:
  - Cumulative savings distributed across all users
  - User distribution across fee tiers
  - Average discount percentage realized across all operations

---

## API Reference

- `GET /api/fee-tiers/tiers`: Retrieve all active tier definitions and criteria.
- `PUT /api/fee-tiers/config`: Update tier thresholds and discount rates (admin-governed).
- `POST /api/fee-tiers/status`: Check a user's current tier, loyalty progress, and lifetime savings.
- `POST /api/fee-tiers/apply`: Apply tier & loyalty discounts to a transaction.
- `POST /api/fee-tiers/transparency`: Itemized fee transparency calculation breakdown.
- `GET /api/fee-tiers/analytics`: Protocol-wide tier statistics and discount analytics.

---

## Frontend Integration
- `frontend/src/components/FeeTierStatus.tsx`: User interface providing tier status overview, progress bars toward the next tier, full tier criteria tables, and interactive fee transparency calculator.
