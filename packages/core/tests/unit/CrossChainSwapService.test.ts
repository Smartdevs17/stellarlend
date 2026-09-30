// SPDX-License-Identifier: Apache-2.0
import { CrossChainSwapService } from '../../src/services/CrossChainSwapService'
import { StellarChainAdapter, EvmChainAdapter } from '../../src/adapters'
import { AtomicSettlement } from '../../src/contracts'

jest.mock('../../src/adapters/StellarChainAdapter')
jest.mock('../../src/adapters/EvmChainAdapter')
jest.mock('../../src/contracts/AtomicSettlement')

describe('CrossChainSwapService', () => {
  let service: CrossChainSwapService
  let mockStellarAdapter: jest.Mocked<StellarChainAdapter>
  let mockEvmAdapter: jest.Mocked<EvmChainAdapter>
  let mockSettlement: jest.Mocked<AtomicSettlement>

  beforeEach(() => {
    mockStellarAdapter = new StellarChainAdapter() as jest.Mocked<StellarChainAdapter>
    mockEvmAdapter = new EvmChainAdapter() as jest.Mocked<EvmChainAdapter>
    mockSettlement = new AtomicSettlement() as jest.Mocked<AtomicSettlement>

    service = new CrossChainSwapService()
    jest.clearAllMocks()
  })

  describe('executeSwap', () => {
    it('should execute atomic swap successfully', async () => {
      const params = {
        sourceChain: 'stellar',
        targetChain: 'evm',
        sourceAsset: 'XLM',
        targetAsset: '0x...',
        amount: 1000n,
        userAddress: 'user_address',
        deadline: Date.now() + 3600
      }

      mockStellarAdapter.buildSwapLockOperation.mockResolvedValue({} as any)
      mockStellarAdapter.submitTransaction.mockResolvedValue('source_tx_hash')
      mockEvmAdapter.buildSwapLockTransaction.mockResolvedValue({ hash: 'target_tx_hash' } as any)
      mockEvmAdapter.submitTransaction.mockResolvedValue('target_tx_hash')
      mockSettlement.createSettlementTransaction.mockResolvedValue({ hash: 'settlement_tx_hash' } as any)

      const result = await service.executeSwap(params)

      expect(result).toEqual({
        sourceTxHash: 'source_tx_hash',
        targetTxHash: 'target_tx_hash',
        settlementTxHash: 'settlement_tx_hash'
      })
    })

    it('should reject if source transaction fails', async () => {
      const params = {
        sourceChain: 'stellar',
        targetChain: 'evm',
        sourceAsset: 'XLM',
        targetAsset: '0x...',
        amount: 1000n,
        userAddress: 'user_address',
        deadline: Date.now() + 3600
      }

      mockStellarAdapter.submitTransaction.mockRejectedValue(new Error('Source failed'))

      await expect(service.executeSwap(params)).rejects.toThrow('Source or target transaction failed')
    })

    it('should reject if timeout occurs', async () => {
      const params = {
        sourceChain: 'stellar',
        targetChain: 'evm',
        sourceAsset: 'XLM',
        targetAsset: '0x...',
        amount: 1000n,
        userAddress: 'user_address',
        deadline: Date.now() + 3600
      }

      mockStellarAdapter.submitTransaction.mockImplementation(() => {
        return new Promise((_, reject) => setTimeout(() => reject(new Error('Timeout')), 10))
      })

      await expect(service.executeSwap(params)).rejects.toThrow('Transaction timeout')
    })
  })
})