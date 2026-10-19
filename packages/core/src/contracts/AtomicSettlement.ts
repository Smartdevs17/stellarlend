// SPDX-License-Identifier: Apache-2.0
import { ethers } from 'ethers'

interface AtomicSettlementParams {
  sourceTxHash: string
  targetTxHash: string
  sourceChain: 'stellar' | 'evm'
  targetChain: 'stellar' | 'evm'
  amount: bigint
  userAddress: string
  deadline: number
}

export class AtomicSettlement {
  private provider: ethers.providers.Provider
  private contractAddress: string
  private contractABI: ethers.ContractInterface

  constructor() {
    this.provider = new ethers.providers.JsonRpcProvider('https://rpc.mainnet.stellar.org')
    this.contractAddress = '0x123...abc'
    this.contractABI = new ethers.utils.Interface([
      'function createSettlement(
        string memory sourceTxHash,
        string memory targetTxHash,
        string memory sourceChain,
        string memory targetChain,
        uint256 amount,
        address userAddress,
        uint256 deadline
      ) external returns (string memory settlementTxHash)',
      'function verifySettlement(
        string memory sourceTxHash,
        string memory targetTxHash,
        string memory sourceChain,
        string memory targetChain,
        uint256 amount,
        address userAddress
      ) external returns (bool)'
    ])
  }

  public async createSettlementTransaction(params: AtomicSettlementParams): Promise<ethers.TransactionResponse> {
    const { sourceTxHash, targetTxHash, sourceChain, targetChain, amount, userAddress, deadline } = params
    const contract = new ethers.Contract(this.contractAddress, this.contractABI, this.provider.getSigner())
    return await contract.createSettlement(
      sourceTxHash,
      targetTxHash,
      sourceChain,
      targetChain,
      amount,
      userAddress,
      deadline
    )
  }

  public async verifySettlement(params: Omit<AtomicSettlementParams, 'deadline'>): Promise<boolean> {
    const { sourceTxHash, targetTxHash, sourceChain, targetChain, amount, userAddress } = params
    const contract = new ethers.Contract(this.contractAddress, this.contractABI, this.provider)
    return await contract.verifySettlement(
      sourceTxHash,
      targetTxHash,
      sourceChain,
      targetChain,
      amount,
      userAddress
    )
  }
}