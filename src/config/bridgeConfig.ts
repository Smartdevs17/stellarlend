import { BridgeConfig } from '../bridge/BridgeRegistry';

export const BRIDGE_CONFIGS: BridgeConfig[] = [
  {
    id: 'wormhole-ethereum',
    name: 'Wormhole Ethereum Bridge',
    type: 'wormhole',
    endpoint: 'https://wormhole-api.example.com',
    supportedChains: ['ethereum', 'stellar', 'solana'],
    feePercentage: 0.001,
    isActive: true,
  },
  {
    id: 'cctp-ethereum',
    name: 'Circle CCTP',
    type: 'cctp',
    endpoint: 'https://cctp-api.example.com',
    supportedChains: ['ethereum', 'avalanche', 'polygon'],
    feePercentage: 0.0005,
    isActive: true,
  },
  {
    id: 'layerzero-ethereum',
    name: 'LayerZero Ethereum Bridge',
    type: 'layerzero',
    endpoint: 'https://layerzero-api.example.com',
    supportedChains: ['ethereum', 'arbitrum', 'optimism'],
    feePercentage: 0.0015,
    isActive: true,
  },
];

export const WRAPPED_ASSETS = [
  {
    canonicalAsset: 'USDC',
    wrappedAsset: 'WUSDC:ethereum',
    bridgeId: 'wormhole-ethereum',
    chainId: 'ethereum',
  },
  {
    canonicalAsset: 'USDC',
    wrappedAsset: 'WUSDC:avalanche',
    bridgeId: 'cctp-ethereum',
    chainId: 'avalanche',
  },
  {
    canonicalAsset: 'USDC',
    wrappedAsset: 'WUSDC:polygon',
    bridgeId: 'cctp-ethereum',
    chainId: 'polygon',
  },
  {
    canonicalAsset: 'USDC',
    wrappedAsset: 'WUSDC:solana',
    bridgeId: 'wormhole-ethereum',
    chainId: 'solana',
  },
];
