// SPDX-License-Identifier: Apache-2.0
export interface CrossChainSwapParams {
  sourceChain: 'stellar' | 'evm'
  targetChain: 'stellar' | 'evm'
  sourceAsset: string
  targetAsset: string
  amount: bigint
  userAddress: string
  deadline: number
}

export interface CrossChainSwapResult {
  sourceTxHash: string
  targetTxHash: string
  settlementTxHash: string
}

export interface AtomicSettlementParams {
  sourceTxHash: string
  targetTxHash: string
  sourceChain: 'stellar' | 'evm'
  targetChain: 'stellar' | 'evm'
  amount: bigint
  userAddress: string
  deadline: number
}