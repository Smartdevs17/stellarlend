import { BridgeRegistry } from '../BridgeRegistry';

describe('BridgeRegistry', () => {
  let registry: BridgeRegistry;

  beforeEach(() => {
    registry = BridgeRegistry.getInstance();
    registry['bridges'].clear();
    registry['wrappedAssets'].length = 0;
  });

  it('should register and retrieve bridges', () => {
    const config = {
      id: 'wormhole-ethereum',
      name: 'Wormhole Ethereum Bridge',
      type: 'wormhole' as const,
      endpoint: 'https://wormhole.example.com',
      supportedChains: ['ethereum', 'stellar'],
      feePercentage: 0.001,
      isActive: true,
    };

    registry.registerBridge(config);
    const retrieved = registry.getBridge('wormhole-ethereum');

    expect(retrieved).toEqual(config);
  });

  it('should register and retrieve wrapped assets', () => {
    const info = {
      canonicalAsset: 'USDC',
      wrappedAsset: 'WUSDC:ethereum',
      bridgeId: 'wormhole-ethereum',
      chainId: 'ethereum',
    };

    registry.registerWrappedAsset(info);
    const retrieved = registry.getWrappedAsset('USDC', 'ethereum');

    expect(retrieved).toEqual(info);
  });

  it('should return supported bridges for a chain', () => {
    const config1 = {
      id: 'wormhole-ethereum',
      name: 'Wormhole Ethereum Bridge',
      type: 'wormhole' as const,
      endpoint: 'https://wormhole.example.com',
      supportedChains: ['ethereum', 'stellar'],
      feePercentage: 0.001,
      isActive: true,
    };

    const config2 = {
      id: 'cctp-ethereum',
      name: 'Circle CCTP',
      type: 'cctp' as const,
      endpoint: 'https://cctp.example.com',
      supportedChains: ['ethereum', 'avalanche'],
      feePercentage: 0.0005,
      isActive: false,
    };

    registry.registerBridge(config1);
    registry.registerBridge(config2);

    const supported = registry.getSupportedBridges('stellar');
    expect(supported).toEqual([config1]);
  });

  it('should return canonical asset for wrapped asset', () => {
    const info = {
      canonicalAsset: 'USDC',
      wrappedAsset: 'WUSDC:ethereum',
      bridgeId: 'wormhole-ethereum',
      chainId: 'ethereum',
    };

    registry.registerWrappedAsset(info);
    const retrieved = registry.getCanonicalAsset('WUSDC:ethereum');

    expect(retrieved).toEqual(info);
  });
});
