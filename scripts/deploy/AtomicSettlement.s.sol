// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

contract AtomicSettlement {
  struct SwapLock {
    string sourceTxHash;
    string targetTxHash;
    string sourceChain;
    string targetChain;
    uint256 amount;
    address userAddress;
    uint256 deadline;
    bool settled;
  }

  mapping(bytes32 => SwapLock) private swapLocks;
  address public owner;

  event SwapSettled(
    string sourceTxHash,
    string targetTxHash,
    string sourceChain,
    string targetChain,
    uint256 amount,
    address userAddress
  );

  constructor() {
    owner = msg.sender;
  }

  modifier onlyOwner() {
    require(msg.sender == owner, 'Not owner');
    _;
  }

  function createSettlement(
    string memory sourceTxHash,
    string memory targetTxHash,
    string memory sourceChain,
    string memory targetChain,
    uint256 amount,
    address userAddress,
    uint256 deadline
  ) external returns (string memory) {
    bytes32 lockId = keccak256(abi.encodePacked(sourceTxHash, targetTxHash, userAddress));
    require(!swapLocks[lockId].settled, 'Already settled');
    require(block.timestamp <= deadline, 'Deadline passed');

    swapLocks[lockId] = SwapLock({
      sourceTxHash,
      targetTxHash,
      sourceChain,
      targetChain,
      amount,
      userAddress,
      deadline,
      settled: true
    });

    emit SwapSettled(sourceTxHash, targetTxHash, sourceChain, targetChain, amount, userAddress);
    return keccak256(abi.encodePacked(
      'settlement_',
      keccak256(abi.encodePacked(sourceTxHash, targetTxHash, userAddress))
    ));
  }

  function verifySettlement(
    string memory sourceTxHash,
    string memory targetTxHash,
    string memory sourceChain,
    string memory targetChain,
    uint256 amount,
    address userAddress
  ) external view returns (bool) {
    bytes32 lockId = keccak256(abi.encodePacked(sourceTxHash, targetTxHash, userAddress));
    SwapLock memory lock = swapLocks[lockId];
    require(lock.settled, 'Not settled');
    require(lock.sourceChain == sourceChain, 'Chain mismatch');
    require(lock.targetChain == targetChain, 'Chain mismatch');
    require(lock.amount == amount, 'Amount mismatch');
    require(lock.userAddress == userAddress, 'User mismatch');
    return true;
  }
}