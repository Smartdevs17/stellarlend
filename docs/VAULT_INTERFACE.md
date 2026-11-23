# ERC-4626 Vault Interface for Stablecoin Pools

This document describes the ERC-4626 Tokenized Vault interface extracted for stablecoin pools in StellarLend (#1608).

## Overview

The ERC-4626 vault interface provides a standardized, tokenized yield-bearing vault layer for stablecoin pools (e.g. USDC, EURC, USDS). By unifying share-to-asset accounting, preview mechanics, and limit queries behind a single standard, external integrations (routers, aggregators, dashboards, and automated strategies) can interact with stablecoin pools through a predictable, battle-tested API.

## Core Interface

The interface follows the standard ERC-4626 specification adapted for Soroban:

### State & Accounting
- `asset(pool)`: Underlying asset contract address.
- `total_assets(pool)`: Total underlying assets managed by the vault (including accrued yield).
- `total_shares(pool)`: Total vault shares minted and outstanding.
- `convert_to_shares(pool, assets)`: Converts assets to shares (rounds down).
- `convert_to_assets(pool, shares)`: Converts shares to assets (rounds down).

### Preview Methods
- `preview_deposit(pool, assets)`: Simulates deposit, returns shares to be minted (rounds down).
- `preview_mint(pool, shares)`: Simulates mint, returns assets required (rounds up).
- `preview_withdraw(pool, assets)`: Simulates withdrawal, returns shares to be burned (rounds up).
- `preview_redeem(pool, shares)`: Simulates redemption, returns assets to be released (rounds down).

### Limit Queries
- `max_deposit(pool, receiver)`: Maximum assets that can be deposited given current caps and pause status.
- `max_mint(pool, receiver)`: Maximum shares that can be minted given current caps and pause status.
- `max_withdraw(pool, owner)`: Maximum assets that can be withdrawn by `owner`.
- `max_redeem(pool, owner)`: Maximum shares that can be redeemed by `owner`.

### Mutations
- `deposit(caller, assets, receiver, pool)`: Deposits exact assets and mints shares to receiver.
- `mint(caller, shares, receiver, pool)`: Deposits required assets to mint exact shares to receiver.
- `withdraw(caller, assets, receiver, owner, pool)`: Burns shares from owner and transfers assets to receiver.
- `redeem(caller, shares, receiver, owner, pool)`: Burns exact shares from owner and transfers assets to receiver.

### Share & Allowance Management
- `vault_share_balance_of(pool, user)`: Balance of vault shares for `user`.
- `vault_share_allowance(pool, owner, spender)`: Approved allowance of shares.
- `vault_approve_shares(owner, spender, pool, amount)`: Grants allowance for spender.
- `vault_transfer_shares(from, to, pool, amount)`: Transfers shares between accounts.

## Security & Rounding

1. **Virtual Offset Inflation Protection**:
   Empty or newly initialized vaults are protected from donation/inflation attacks via a virtual offset of 1 (`VIRTUAL_OFFSET = 1`):
   $$\text{shares} = \frac{\text{assets} \times (\text{total\_shares} + 1)}{\text{total\_assets} + 1}$$
   $$\text{assets} = \frac{\text{shares} \times (\text{total\_assets} + 1)}{\text{total\_shares} + 1}$$

2. **Rounding Direction**:
   - Deposits and redemptions round in favor of the vault (shares minted round down, assets released round down).
   - Mints and withdrawals round in favor of the vault (assets required to deposit round up, shares burned round up).

## Observability

All lifecycle actions publish typed contract events:
- `VaultDepositEvent`: `(pool, caller, receiver, assets, shares, timestamp)`
- `VaultWithdrawEvent`: `(pool, caller, receiver, owner, assets, shares, timestamp)`
- `VaultConfiguredEvent`: `(pool, caller, deposit_cap, deposit_paused, withdraw_paused, timestamp)`
- `VaultApprovalEvent`: `(pool, owner, spender, amount, timestamp)`
- `VaultYieldAccruedEvent`: `(pool, yield_amount, new_total_assets, timestamp)`
