# Lending Pool Yield Aggregator with Best-Rate Routing

## Overview

The StellarLend Yield Aggregator continuously monitors and aggregates supply yields, borrow rates, and rewards across native and partner pools on Stellar/Soroban (including StellarLend Core, Blend Protocol, Soroswap AMM pools, and Aquarius LP). It provides algorithmic best-rate routing to eliminate manual capital re-allocation and maximize net yield for lenders.

---

## Features

### 1. Yield Aggregation Across Pools
- Aggregates multi-protocol yield metrics for supported Stellar assets (USDC, XLM, EURC, BTC).
- Tracks base supply APY, borrow APY, protocol reward APY, and blended net APY.
- Incorporates 7-day and 30-day moving averages and historical yield volatility.

### 2. Best-Rate Routing Algorithm
- **Convex Allocation Optimization**:
  - For small deposits ($< 1\%$ pool TVL), capital is 100% routed to the highest net APY pool.
  - For large deposits, a single pool's rate curve will suffer dilution:
    $$U_{new} = \frac{\text{Borrows}}{\text{Supply} + \Delta\text{Deposit}}$$
    Supplying excessive capital to one pool compresses its utilization and drops the APY.
  - The routing algorithm solves for the optimal multi-pool split across up to 3 candidate pools to maximize total annual dollar yield while keeping gas execution overhead bounded.
- **Routing Strategies**:
  - `highest_yield`: Pure APY maximization with dilution modeling.
  - `balanced_risk`: Weights APY against pool risk score (1-10) to favor blue-chip pools.
  - `gas_optimized`: Minimizes multi-contract calls by prioritizing native single-pool allocation.

### 3. Yield Comparison Tools
- Side-by-side metric comparison across candidate pools.
- Risk-adjusted return calculation ($\frac{\text{Net APY}}{\text{Risk Score}}$).
- Maximum deposit capacity without exceeding 0.5% APY slippage.

### 4. Yield Analytics
- 30-day historical time-series of pool rates.
- Utilization vs. supply rate curves for each pool.
- Volatility index and Sharpe ratios for yield consistency.

### 5. Yield Alerts
- Real-time subscription engine for lenders.
- Conditions:
  - `above`: Triggers when net APY exceeds target threshold.
  - `below`: Triggers warning when pool yield drops below acceptable floor.
  - `opportunity_gain`: Triggers when an alternative pool offers $>100\text{ bps}$ higher APY.

---

## API Reference

- `GET /api/yield-aggregator/pools`: List all active pools (optional `?asset=USDC` filter).
- `POST /api/yield-aggregator/route`: Execute best-rate routing for asset & amount.
- `POST /api/yield-aggregator/compare`: Compare selected pool IDs or assets side-by-side.
- `GET /api/yield-aggregator/analytics/:poolId`: Retrieve 30-day history and interest rate curves.
- `POST /api/yield-aggregator/alerts`: Create yield alert subscription.
- `GET /api/yield-aggregator/alerts/:userId`: List user's active alerts.
- `DELETE /api/yield-aggregator/alerts/:id`: Remove alert subscription.
- `GET /api/yield-aggregator/alerts/check`: Evaluate alert triggers across all subscriptions.

---

## Frontend Component
- `frontend/src/components/YieldAggregatorDashboard.tsx`: Complete dashboard featuring the Best-Rate Router, Aggregated Pools table, Side-by-side comparison, and Yield Alerts creation.
