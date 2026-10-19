// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/math/SafeMath.sol";

contract Delegation is Ownable {
    using SafeMath for uint256;

    struct DelegateInfo {
        address delegate;
        uint256 votingPower;
        uint256 timestamp;
    }

    mapping(address => DelegateInfo) public delegates;
    mapping(address => uint256) public votingPower;
    mapping(address => bool) public isDelegated;

    event DelegateSet(address indexed delegator, address indexed delegate, uint256 votingPower);
    event DelegateRemoved(address indexed delegator, address indexed delegate);

    modifier notSelf(address _delegate) {
        require(_delegate != msg.sender, "Cannot delegate to self");
        _;
    }

    modifier notCircular(address _delegate) {
        require(!isDelegated[_delegate], "Circular delegation detected");
        _;
    }

    function delegate(address _delegate, uint256 _votingPower)
        external
        notSelf(_delegate)
        notCircular(_delegate)
    {
        require(_votingPower > 0, "Voting power must be positive");
        require(votingPower[msg.sender] >= _votingPower, "Insufficient voting power");

        delegates[msg.sender] = DelegateInfo({
            delegate: _delegate,
            votingPower: _votingPower,
            timestamp: block.timestamp
        });

        isDelegated[msg.sender] = true;
        votingPower[msg.sender] = votingPower[msg.sender].sub(_votingPower);
        votingPower[_delegate] = votingPower[_delegate].add(_votingPower);

        emit DelegateSet(msg.sender, _delegate, _votingPower);
    }

    function undelegate() external {
        DelegateInfo storage delegateInfo = delegates[msg.sender];
        require(delegateInfo.delegate != address(0), "No delegate set");

        address delegate = delegateInfo.delegate;
        uint256 power = delegateInfo.votingPower;

        votingPower[msg.sender] = votingPower[msg.sender].add(power);
        votingPower[delegate] = votingPower[delegate].sub(power);

        delete delegates[msg.sender];
        isDelegated[msg.sender] = false;

        emit DelegateRemoved(msg.sender, delegate);
    }

    function getDelegatedVotingPower(address _account) external view returns (uint256) {
        return delegates[_account].votingPower;
    }

    function getDelegate(address _account) external view returns (address) {
        return delegates[_account].delegate;
    }
}