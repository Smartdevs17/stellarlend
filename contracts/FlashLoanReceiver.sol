// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

import "./ReentrancyGuard.sol";
import "./interfaces/IFlashLoanReceiver.sol";

contract FlashLoanReceiver is ReentrancyGuard, IFlashLoanReceiver {
    // ... existing state variables ...
    
    /**
     * @dev Execute flash loan operations
     * @dev Uses nonReentrant modifier to prevent reentrancy
     */
    function executeOperation(
        address asset,
        uint256 amount,
        uint256 premium,
        address initiator,
        bytes calldata params
    ) external nonReentrant override {
        // Check: Validate flash loan parameters
        _validateFlashLoan(asset, amount, premium, initiator, params);
        
        // Effect: Process flash loan
        _processFlashLoan(asset, amount, params);
        
        // Interaction: Repay premium
        IERC20(asset).safeTransfer(address(this), premium);
    }
    
    // Internal validation and processing functions
    function _validateFlashLoan(
        address asset,
        uint256 amount,
        uint256 premium,
        address initiator,
        bytes calldata params
    ) internal view {
        // ... validation logic ...
    }
    
    function _processFlashLoan(
        address asset,
        uint256 amount,
        bytes calldata params
    ) internal {
        // ... processing logic ...
    }
}