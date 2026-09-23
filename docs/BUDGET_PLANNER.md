# Lending Protocol Budget Planner

## Overview

The **Lending Protocol Budget Planner** empowers liquidity providers and lenders to plan capital allocation across lending pools, calculate compound yield projections across multiple horizons and market scenarios, integrate institutional-grade risk assessment, auto-optimize allocations according to financial objectives, track budget performance against actual returns, and configure automated alerts.

---

## Key Features

### 1. Budget Planning Interface
- Dynamic multi-pool weight adjustment with basis points precision (10,000 bps = 100%).
- Real-time capacity checks and validation preventing overallocation into saturated pools.
- Sequential action generator creating ready-to-execute deposit steps.

### 2. Yield Projection Calculator
- Compounding frequencies:
  - **Simple interest**: $P \cdot (1 + r \cdot \frac{t}{365})$
  - **Daily compounding**: $P \cdot (1 + \frac{r}{365})^{365 \cdot \frac{t}{365}}$
  - **Monthly compounding**: $P \cdot (1 + \frac{r}{12})^{12 \cdot \frac{t}{365}}$
  - **Annual compounding**: $P \cdot (1 + r)^{\frac{t}{365}}$
- Multi-horizon evaluation (30d, 90d, 180d, 365d, 730d, 1095d).
- Market scenario modeling:
  - **Conservative**: Base APY $\times 0.8$
  - **Base**: Base APY $\times 1.0$
  - **Optimistic**: Base APY $\times 1.25$
- Net yield estimation accounting for protocol fees.

### 3. Risk Assessment Integration
- Quantitative scoring per risk grade (AAA: 5, AA: 12, A: 25, BBB: 40, BB: 55, B: 70, C: 85, D: 95).
- Composite portfolio risk score (0–100 scale) and qualitative rating (`Low`, `Moderate`, `Elevated`, `High`).
- Herfindahl-Hirschman Index (HHI) concentration metric and diversification score.
- Stress scenario maximum drawdown estimation (simulated haircuts on lower-grade collateral).
- Warnings for excessive concentration or overexposure to speculative pools.

### 4. Budget Optimization Engine
- Three automated optimization strategies:
  1. **`max_yield`**: Greedily allocates capital to the highest-yielding pools within risk grade caps and pool capacities.
  2. **`min_risk`**: Minimizes overall portfolio volatility by weighting inversely to credit risk scores.
  3. **`balanced`**: Sharpe-optimal risk-adjusted allocation where allocation weight is proportional to $\frac{\text{APY}}{\text{Risk Score}}$.
- Automatic rebalance identification when APY spreads between pools exceed the configured threshold.

### 5. Budget Tracking & Variance
- Saved budget plan persistence with unique plan IDs.
- Periodic tracking of actual realized returns vs. projected returns.
- Variance drift calculation in basis points and percentage.
- Lifecycle status tracking: `on_track`, `underperforming`, or `outperforming`.

### 6. Budget Alerts
- Configurable alerts:
  - **Yield drop**: Triggered when pool APY falls below a defined basis points floor.
  - **Risk budget breach**: Alert when grade allocation exceeds user limit.
  - **Variance drift**: Alert when actual yield drifts negatively from forecast by more than $X\%$.
  - **Rebalance needed**: Triggered when a more lucrative rebalancing route opens.

---

## API Reference

### Build Plan
```http
POST /api/planner/budget
Content-Type: application/json

{
  "capital": 10000,
  "horizonDays": 365,
  "goalAmount": 11000,
  "rebalanceThresholdBps": 100,
  "maxRiskExposureBps": { "A": 10000, "B": 5000 },
  "compoundingFrequency": "monthly",
  "pools": [
    { "poolId": "pool-usdc", "weightBps": 6000, "apyBps": 800, "riskGrade": "A", "capacity": 100000 },
    { "poolId": "pool-xlm", "weightBps": 4000, "apyBps": 500, "riskGrade": "A", "capacity": 50000 }
  ]
}
```

### Calculate Yield Projections
```http
POST /api/planner/yield-projections
Content-Type: application/json

{
  "capital": 10000,
  "horizonDays": 365,
  "compoundingFrequency": "daily",
  "pools": [ ... ]
}
```

### Optimize Allocations
```http
POST /api/planner/optimize
Content-Type: application/json

{
  "capital": 10000,
  "strategy": "balanced",
  "pools": [ ... ]
}
```

### Save Plan for Tracking
```http
POST /api/planner/plans
Content-Type: application/json

{
  "userAddress": "G...",
  "name": "Q3 2026 Yield Plan",
  "capital": 10000,
  "horizonDays": 365,
  "pools": [ ... ]
}
```

### Record Actual Returns
```http
POST /api/planner/plans/:id/record-actual
Content-Type: application/json

{
  "actualReturn": 650.50
}
```

### Configure Alert
```http
POST /api/planner/alerts
Content-Type: application/json

{
  "userAddress": "G...",
  "alert": {
    "type": "yield_drop",
    "threshold": 300,
    "poolId": "pool-usdc",
    "enabled": true
  }
}
```
