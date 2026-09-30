import { BridgeService } from '../BridgeService';
import { BridgeRegistry } from '../BridgeRegistry';
import { StellarAssetService } from '../../services/StellarAssetService';

jest.mock('../BridgeRegistry');
jest.mock('../../services/StellarAssetService');

describe('BridgeService', () => {
  let service: BridgeService;
  let mockRegistry: jest.Mocked<BridgeRegistry>;
  let mockStellarService: jest.Mocked<StellarAssetService>;

  beforeEach(() => {
    mockRegistry = new (BridgeRegistry as any)();
    mockStellarService = new (StellarAssetService as any)();
    service = new BridgeService(mockStellarService);
    (BridgeRegistry.getInstance as jest.Mock).mockReturnValue(mockRegistry);
  });

  it('should initiate cross-chain transfer with optimal bridge', async () => {
    const request = {
      sourceChain: 'stellar',
      destinationChain: 'ethereum',
      asset: 'USDC',
      amount: '100000000',
      recipient: '0x123456789',
    };

    const bridgeConfig = {
      id: 'wormhole-ethereum',
      name: 'Wormhole Ethereum Bridge',
      type: 'wormhole' as const,
      endpoint: 'https://wormhole.example.com',
      supportedChains: ['ethereum', 'stellar'],
      feePercentage: 0.001,
      isActive: true,
    };

    const wrappedAsset = {
      canonicalAsset: 'USDC',
      wrappedAsset: 'WUSDC:ethereum',
      bridgeId: 'wormhole-ethereum',
      chainId: 'ethereum',
    };

    mockRegistry.getSupportedBridges.mockReturnValue([bridgeConfig]);
    mockRegistry.getWrappedAsset.mockReturnValue(wrappedAsset);

    const result = await service.initiateCrossChainTransfer(request);

    expect(result.status).toBe('pending');
    expect(result.bridgeId).toBe('wormhole-ethereum');
    expect(result.wrappedAsset).toBe('WUSDC:ethereum');
    expect(result.estimatedFee).toBe('100000');
  });

  it('should throw error when no bridges are available', async () => {
    const request = {
      sourceChain: 'stellar',
      destinationChain: 'ethereum',
      asset: 'USDC',
      amount: '100000000',
      recipient: '0x123456789',
    };

    mockRegistry.getSupportedBridges.mockReturnValue([]);

    await expect(service.initiateCrossChainTransfer(request)).rejects.toThrow(
      'No supported bridges available for this chain'
    );
  });

  it('should handle bridge failure', async () => {
    const consoleWarnSpy = jest.spyOn(console, 'warn').mockImplementation();

    await service.handleBridgeFailure('test-tx-id');

    expect(consoleWarnSpy).toHaveBeenCalledWith(
      'Handling bridge failure for transaction: test-tx-id'
    );

    consoleWarnSpy.mockRestore();
  });
});
