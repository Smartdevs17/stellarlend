// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

import "@openzeppelin/contracts/utils/Context.sol";

/**
 * @title ReentrancyGuard
 * @dev Contract module that helps prevent reentrancy attacks
 */
contract ReentrancyGuard is Context {
    uint256 private constant _NOT_ENTERED = 1;
    uint256 private constant _ENTERED = 2;
    
    uint256 private _status;
    
    event ReentrancyGuardEntered();
    event ReentrancyGuardExited();
    
    /**
     * @dev Modifier to protect against reentrancy
     * @dev Follows Check-Effects-Interactions pattern
     */
    modifier nonReentrant() {
        require(_status == _NOT_ENTERED, "ReentrancyGuard: reentrant call");
        _status = _ENTERED;
        _;
        _status = _NOT_ENTERED;
        emit ReentrancyGuardExited();
    }
    
    /**
     * @dev Returns true if the contract has reentrancy protection enabled
     */
    function isReentrant() public view returns (bool) {
        return _status == _ENTERED;
    }
}