# Glossary of Protocol Terms

Definitions for the terms used across the StellarLend contracts, API, oracle,
and documentation. Terms are listed alphabetically; where a term has its own
document, the link is given.

| Term | Definition |
| --- | --- |
| **AMM** | Automated Market Maker. An on-chain pool that prices swaps with a formula instead of an order book. StellarLend integrates AMM hooks for swaps and liquidity operations; see `contracts/amm`. |
| **Basis points (bps)** | A fixed-point unit where 10,000 bps = 100%. Protocol ratios such as the minimum collateral ratio are expressed in bps. |
| **Borrow** | Drawing assets from the lending contract against deposited collateral. Debt accrues interest from the moment it is borrowed. |
| **Bridge** | The cross-chain transfer interface. Admins register a network with a bridge address and a `fee_bps`; `bridge_deposit`/`bridge_withdraw` move balances and emit events carrying the fee. |
| **Collateral** | Assets deposited into the protocol that back a borrow. Collateral is locked while it backs debt and can be withdrawn only while the position stays above the minimum collateral ratio. |
| **Collateral ratio** | The value of a position's collateral relative to its debt, in bps. The borrow path enforces a configurable floor (`COLLATERAL_RATIO_MIN`, currently 15,000 bps = 150% in the lending contract). |
| **Cross-asset position** | A position whose collateral and debt may be denominated in different supported assets. Valuation converts each side through oracle prices; see `contracts/lending` and `cross_asset.md`. |
| **Debt token** | A token representing a lender's claim or a borrower's debt in a modular pool; see `stellar-lend/contracts/debt-token` and `principal-token`. |
| **Deposit** | Adding assets to the lending contract (to earn interest) or adding collateral to a position. Deposits emit events and update analytics immediately. |
| **Flash loan** | An uncollateralized loan that must be borrowed and repaid within a single transaction. A configurable fee applies; the repayment is checked atomically, so an unprofitable but repaying borrower is free to use the funds. See `flash_loan.md`. |
| **Guardian** | An address a user nominates to approve a social-recovery action on their account, subject to a timelock and threshold. |
| **Health factor** | The safety measure of a position, scaled so that 10,000 = 1.0. Above 10,000 the position is healthy; at or below it, the position is liquidatable. |
| **Interest rate model** | The formula that maps utilization to the borrow rate, with smoothing to avoid abrupt jumps. See `interest_rate_models.md`. |
| **Liquidation** | Forcing the repayment of an unhealthy position: a liquidator repays up to a capped amount of debt and receives collateral plus a liquidation incentive. The health factor must be at or below the threshold. See `liquidation.md`. |
| **Liquidation incentive** | The discount a liquidator receives for closing an unhealthy position, which compensates them for the risk and keeps the protocol solvent. |
| **Liquidity ratio** | The share of deposited assets still available to withdraw. Below the configured floor, withdrawals and certain actions are blocked to avoid a run. |
| **Minimum collateral ratio** | The lowest collateral-to-debt ratio a position may have after an action; below it, borrows and collateral withdrawals are rejected. |
| **Multisig** | An admin mechanism where several signers must approve a privileged action (for example a minimum-collateral change) before it executes. See `multisig.md`. |
| **Oracle** | The service and contracts that supply asset prices used for valuation. StellarLend uses anomaly detection, a price cache with TTL, and TWAP smoothing. See `REALTIME_PRICE_FEED.md` and `twap-oracle-security.md`. |
| **Pause switch** | An admin (or guardian) toggle that stops one operation class (for example withdrawals) without stopping the whole protocol; see `pause.md`. |
| **Position** | The aggregate state of one user in the lending contract: collateral balances, debt balances, accrued interest, and derived values (values, health factor). |
| **Rate guard** | A circuit breaker that rejects state-changing calls when a sanity limit (for example an implausible oracle move) is crossed; see `rate-limit` and `utilization_breaker`. |
| **Repay** | Returning borrowed assets plus accrued interest, which reduces the debt and improves the health factor. |
| **Risk monitor** | The monitoring subsystem that evaluates health, performance, and security thresholds and emits alerts; see `RISK_MONITORING_DASHBOARD.md`. |
| **Risk score** | A derived score that summarizes a position's or the protocol's risk based on collateralization, utilization, and activity. |
| **Social recovery** | A workflow where guardians approve a recovery request, a timelock elapses, and the account's control is restored; see `recovery.md`. |
| **TVL (Total Value Locked)** | The cumulative value of assets held by the protocol, aggregated across assets in the protocol's smallest units. |
| **TWAP** | Time-Weighted Average Price. An average of oracle prices over a window, used to reduce manipulation and single-block spikes. |
| **Utilization** | The fraction of supplied assets that are borrowed, scaled in the analytics fixed-point convention (1e6 = 100% in report payloads). It drives interest rates and triggers breakers at configured bounds. |
| **Withdraw** | Removing supplied assets or collateral from the lending contract. Collateral withdrawals are rejected when they would push the position below the minimum collateral ratio. |

## Fixed-point conventions

The protocol uses integer fixed-point values for ratios and percentages:

- **Core contract ratios** (health factor) use **10,000 = 1.0**.
- **Analytics/report payloads** expose percentage-style fields scaled by
  **1e6** (`333333` ≈ 33.3333%); see [`docs/README.md`](README.md#analytics-read-apis).

When reading a value, check the field's documented scale before dividing. The
analytics scale is stated explicitly in report field tables; contract ratios use
the 10,000 convention.
