# Cross-Chain Collateral Swaps

## Overview
Enable atomic swaps of collateral assets between supported chains with guaranteed settlement.

## API Reference

### `CrossChainSwapService`

```typescript
import { CrossChainSwapService } from '@stellarlend/core'

const service = new CrossChainSwapService()

interface SwapParams {
  sourceChain: 'stellar' | 'evm'
  targetChain: 'stellar' | 'evm'
  sourceAsset: string
  targetAsset: string
  amount: bigint
  userAddress: string
  deadline: number
}

interface SwapResult {
  sourceTxHash: string
  targetTxHash: string
  settlementTxHash: string
}

// Execute a cross-chain swap
const result: SwapResult = await service.executeSwap(params)
```

## Usage Example

```typescript
// Swap 1000 XLM to ETH on Ethereum
const params = {
  sourceChain: 'stellar',
  targetChain: 'evm',
  sourceAsset: 'XLM',
  targetAsset: '0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2', // WETH
  amount: 1000n,
  userAddress: 'user_stellar_address',
  deadline: Date.now() + 3600
}

const result = await service.executeSwap(params)
console.log('Swap completed:', result)
```

## Atomic Settlement Guarantee

All swaps are protected by an atomic settlement contract that:

1. Requires both source and target transactions to complete
2. Reverts all state changes if any transaction fails
3. Provides cryptographic proof of settlement

## Supported Chains

- Stellar (Soroban)
- Ethereum (EVM)

## Error Handling

The service will reject with:
- `TransactionTimeoutError` if any transaction takes too long
- `SwapFailedError` if source or target transaction fails
- `SettlementVerificationError` if settlement cannot be verified
```