// SPDX-License-Identifier: Apache-2.0
import { CrossChainSwapService } from '../../src/services/CrossChainSwapService'
import { StellarChainAdapter } from '../../src/adapters'
import { AtomicSettlement } from '../../src/contracts'

describe('CrossChainSwapIntegration', () => {
  let service: CrossChainSwapService
  let stellarAdapter: StellarChainAdapter
  let settlementContract: AtomicSettlement

  beforeAll(() => {
    service = new CrossChainSwapService()
    stellarAdapter = new StellarChainAdapter()
    settlementContract = new AtomicSettlement()
  })

  it('should complete end-to-end swap with atomic settlement', async () => {
    const params = {
      sourceChain: 'stellar',
      targetChain: 'evm',
      sourceAsset: 'XLM',
      targetAsset: '0x...',
      amount: 1000n,
      userAddress: 'user_address',
      deadline: Date.now() + 3600
    }

    // This test would use testnet configurations
    // and mock external dependencies in a real implementation
    const result = await service.executeSwap(params)

    // Verify all transactions were submitted
    expect(result.sourceTxHash).toBeDefined()
    expect(result.targetTxHash).toBeDefined()
    expect(result.settlementTxHash).toBeDefined()

    // Verify settlement can be verified
    const isValid = await settlementContract.verifySettlement({
      ...params,
      sourceTxHash: result.sourceTxHash,
      targetTxHash: result.targetTxHash
    })
    expect(isValid).toBe(true)
  })
})