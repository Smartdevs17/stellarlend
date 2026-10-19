// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

import "./ReentrancyGuard.sol";
import "./interfaces/ILendPool.sol";

contract LendPool is ReentrancyGuard, ILendPool {
    // ... existing state variables ...
    
    /**
     * @dev Deposit funds into the pool
     * @param asset Asset address
     * @param amount Amount to deposit
     * @param onBehalfOf User address
     */
    function deposit(
        address asset,
        uint256 amount,
        address onBehalfOf
    ) external nonReentrant override {
        require(amount > 0, "Amount must be greater than 0");
        
        // Check: Validate deposit parameters
        _validateDeposit(asset, amount, onBehalfOf);
        
        // Effect: Update state
        _updateUserBalance(asset, onBehalfOf, amount);
        _updatePoolReserves(asset, amount);
        
        // Interaction: Emit events
        emit Deposit(asset, amount, onBehalfOf, _getTimestamp());
    }
    
    /**
     * @dev Withdraw funds from the pool
     * @param asset Asset address
     * @param amount Amount to withdraw
     * @param to Recipient address
     */
    function withdraw(
        address asset,
        uint256 amount,
        address to
    ) external nonReentrant override {
        require(amount > 0, "Amount must be greater than 0");
        
        // Check: Validate withdrawal parameters
        _validateWithdrawal(asset, amount, to);
        
        // Effect: Update state
        _updateUserBalance(asset, to, -amount);
        _updatePoolReserves(asset, -amount);
        
        // Interaction: Transfer funds
        IERC20(asset).safeTransfer(to, amount);
    }
    
    // ... other protected functions with nonReentrant modifier ...
    
    // Internal validation functions
    function _validateDeposit(
        address asset,
        uint256 amount,
        address onBehalfOf
    ) internal view {
        // ... validation logic ...
    }
    
    function _validateWithdrawal(
        address asset,
        uint256 amount,
        address to
    ) internal view {
        // ... validation logic ...
    }
}