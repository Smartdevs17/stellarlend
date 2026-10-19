// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

import "./ReentrancyGuard.sol";
import "./interfaces/IBorrowPool.sol";

contract BorrowPool is ReentrancyGuard, IBorrowPool {
    // ... existing state variables ...
    
    /**
     * @dev Borrow funds from the pool
     * @param assetCollateral Collateral asset
     * @param assetBorrowed Borrowed asset
     * @param amount Amount to borrow
     * @param interestRate Interest rate
     * @param onBehalfOf User address
     */
    function borrow(
        address assetCollateral,
        address assetBorrowed,
        uint256 amount,
        uint256 interestRate,
        address onBehalfOf
    ) external nonReentrant override {
        require(amount > 0, "Amount must be greater than 0");
        
        // Check: Validate borrow parameters
        _validateBorrow(
            assetCollateral,
            assetBorrowed,
            amount,
            interestRate,
            onBehalfOf
        );
        
        // Effect: Update state
        _updateUserBorrowBalance(assetBorrowed, onBehalfOf, amount);
        _updateUserCollateralBalance(assetCollateral, onBehalfOf, amount);
        _updatePoolReserves(assetBorrowed, amount);
        
        // Interaction: Transfer borrowed funds
        IERC20(assetBorrowed).safeTransfer(onBehalfOf, amount);
    }
    
    /**
     * @dev Repay borrowed funds
     * @param asset Asset to repay
     * @param amount Amount to repay
     * @param onBehalfOf User address
     */
    function repay(
        address asset,
        uint256 amount,
        address onBehalfOf
    ) external nonReentrant override {
        require(amount > 0, "Amount must be greater than 0");
        
        // Check: Validate repayment parameters
        _validateRepayment(asset, amount, onBehalfOf);
        
        // Effect: Update state
        _updateUserBorrowBalance(asset, onBehalfOf, -amount);
        _updatePoolReserves(asset, -amount);
        
        // Interaction: Transfer repaid funds
        IERC20(asset).safeTransferFrom(msg.sender, address(this), amount);
    }
    
    // ... other protected functions with nonReentrant modifier ...
    
    // Internal validation functions
    function _validateBorrow(
        address assetCollateral,
        address assetBorrowed,
        uint256 amount,
        uint256 interestRate,
        address onBehalfOf
    ) internal view {
        // ... validation logic ...
    }
    
    function _validateRepayment(
        address asset,
        uint256 amount,
        address onBehalfOf
    ) internal view {
        // ... validation logic ...
    }
}