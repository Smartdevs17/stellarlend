# Lending Pool Position Health Simulation Tool

## Overview

The Position Health Simulation Tool allows lenders and borrowers on StellarLend to simulate their position health under arbitrary future scenarios before executing transactions on-chain. This replaces static point-in-time checks with comprehensive scenario modeling, what-if analysis, sensitivity curves, and collaborative scenario sharing.

---

## Features

### 1. Position Health Simulation
- **Scenario Parameters**:
  - `price_change_bps`: Collateral price drop or surge in basis points (-80% to +80%).
  - `deposit_amount`: Additional collateral injected into the position.
  - `withdraw_amount`: Planned collateral withdrawal.
  - `borrow_amount`: Planned debt expansion.
  - `repay_amount`: Planned debt deleverage.
- **On-Chain & Off-Chain Precision**:
  - Exact health factor projection: $\text{Health Factor} = \frac{\text{Simulated Collateral} \times 10,000}{\text{Simulated Debt}}$
  - Categorization into risk levels 1 (Safe, $\ge 1.5\text{x}$) to 5 (Critical, $< 1.05\text{x}$).

### 2. Scenario Modeling & Stress Testing
Pre-configured multi-scenario matrices evaluate positions against:
- **Mild Market Dip (-10%)**
- **Moderate Market Correction (-25%)**
- **Severe Flash Crash (-40%)**
- **Black Swan Liquidation Test (-60%)**
- **Collateral Rebalancing Strategies** (+25% deposit, -50% repayment)

### 3. What-If Analysis
Computes actionable thresholds:
- **Liquidation Price**: Exact underlying asset price where health factor reaches 1.0x.
- **Max Safe Withdrawal**: Highest collateral amount that can be withdrawn while keeping health factor above safety thresholds.
- **Max Safe Borrow**: Maximum additional debt allowed without risking liquidation.
- **Target Health Factor Restorer**: Calculates the exact deposit or repayment needed to restore a position to a healthy 1.5x target.

### 4. Simulation Visualization
- **Health Factor Sensitivity Curves**: SVG-based dynamic curve depicting health factor response across -60% to +40% market shifts.
- **Zone Indicators**: Color-coded gauges displaying liquidation proximity (Green $\ge 1.5\text{x}$, Yellow $\ge 1.2\text{x}$, Orange $\ge 1.05\text{x}$, Red $< 1.05\text{x}$).

### 5. Simulation Sharing & Collaboration
- **Persistent Links & Tokens**: Generates shareable URL tokens with 30-day persistence.
- **Scenario Import**: Teammates, DAO risk analysts, and liquidators can import and review shared scenario parameters.

---

## Technical Scope & Architecture

### Smart Contract (`stellar-lend/contracts/hello-world/src/analytics.rs`)
- `PositionSimulationScenario`: Contract type defining scenario inputs.
- `PositionSimulationResult`: Contract type returning simulated collateral, debt, health factor, risk level, liquidation drop percentage, and safe withdrawal/borrow amounts.
- `simulate_position_health(env, user, scenario)`: Simulates health for an active borrower on-chain.
- `simulate_what_if(env, collateral, debt, scenario)`: Pure what-if analysis taking arbitrary collateral and debt inputs.
- Exposed via Soroban contract interface in `lib.rs`.

### Backend API (`api/src/routes/simulation.ts` & `api/src/controllers/simulation.controller.ts`)
- `POST /api/simulation/position`: Single-position scenario simulation.
- `POST /api/simulation/scenario`: Multi-scenario stress-testing matrix.
- `POST /api/simulation/what-if`: Sensitivity threshold analysis.
- `POST /api/simulation/share`: Export and store a shareable simulation token.
- `GET /api/simulation/share/:id`: Retrieve a shared simulation.
- `POST /api/simulation/compare`: Side-by-side scenario comparison.
- `POST /api/simulation/batch`: Batch evaluate multiple positions across pools.

### Frontend UI (`frontend/src/components/PositionSimulator.tsx`)
- Tabbed interface featuring What-If Analysis, Health Visualization curve, Basic/Complex scenarios, Historical replay, and Share Simulation.
