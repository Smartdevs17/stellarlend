# Lending Pool Performance Tracking with Historical Returns

## Overview

The Pool Performance Tracking module provides transparent historical tracking, APY/APR calculations, return metrics, benchmark comparisons, and visualization tools for StellarLend lending pools.

---

## Features

### 1. Historical Performance Tracking
- Captures periodic snapshots of:
  - Total Value Locked (TVL)
  - Utilization rate
  - Supply APY & Supply APR
  - Borrow APY & Borrow APR
  - Cumulative bad debt and recovery events
- Rolling period windows: 7 days, 30 days, 90 days, 1 year.

### 2. APY & APR Calculations
Provides exact mathematical conversion between nominal rates (APR) and compounded yields (APY):
- **Discrete Compounding**:
  $$\text{APY} = \left(1 + \frac{\text{APR}}{n}\right)^n - 1$$
  $$\text{APR} = n \cdot \left((1 + \text{APY})^{1/n} - 1\right)$$
  Supports daily ($n=365$), weekly ($n=52$), monthly ($n=12$), and ledger-close ($n=6,307,200$) compounding frequencies.
- **Continuous Compounding**:
  $$\text{APY} = e^{\text{APR}} - 1$$
  $$\text{APR} = \ln(1 + \text{APY})$$
- **Stellar Ledger Discrete Rates**:
  Translates Soroban contract per-ledger rate increments (~5 seconds close time) into standardized annual percentages.

### 3. Historical Return & Risk Calculations
- **Cumulative Return**: Compounded growth of capital over selected timeframe.
- **Annualized Return**: Scaled annual growth rate.
- **Sharpe Ratio**: Risk-adjusted return over 2% benchmark risk-free rate:
  $$\text{Sharpe} = \frac{\text{Annualized Return} - R_f}{\sigma_{\text{annualized}}}$$
- **Maximum Drawdown (MDD)**: Peak-to-trough decline over the period.

### 4. Performance Comparison & Benchmarks
- Ranks pools by Sharpe ratio, net yield, and TVL.
- Direct benchmarking against external DeFi protocols (Compound and Aave).

---

## API Reference

- `GET /api/pool-performance/snapshots/:poolAddress`: Historical snapshots over period.
- `GET /api/pool-performance/metrics/:poolAddress`: Aggregated metrics (volatility, Sharpe, returns).
- `GET /api/pool-performance/returns/:poolAddress`: Detailed return analysis (cumulative, annualized, drawdown).
- `GET /api/pool-performance/apr-apy-calculator`: Real-time APR <-> APY conversion endpoint.
- `GET /api/pool-performance/compare`: Multi-pool comparison matrix.
- `GET /api/pool-performance/charts/:poolAddress`: Chart series for cumulative return and APY trajectories.
- `GET /api/pool-performance/export/:poolAddress`: Export historical performance as CSV or JSON.

---

## Frontend Visualization
- `frontend/src/components/PerformanceDashboard.tsx`: Interactive performance dashboard including overview metrics, charts, comparison table, heatmap, benchmarks, and integrated APY/APR conversion calculator.
