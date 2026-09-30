import { Asset, Horizon } from 'stellar-sdk';

export interface BridgeConfig {
  id: string;
  name: string;
  type: 'wormhole' | 'cctp' | 'layerzero';
  endpoint: string;
  supportedChains: string[];
  feePercentage: number;
  isActive: boolean;
}

export interface WrappedAssetInfo {
  canonicalAsset: string;
  wrappedAsset: string;
  bridgeId: string;
  chainId: string;
}

export class BridgeRegistry {
  private static instance: BridgeRegistry;
  private bridges: Map<string, BridgeConfig> = new Map();
  private wrappedAssets: WrappedAssetInfo[] = [];

  private constructor() {}

  public static getInstance(): BridgeRegistry {
    if (!BridgeRegistry.instance) {
      BridgeRegistry.instance = new BridgeRegistry();
    }
    return BridgeRegistry.instance;
  }

  public registerBridge(config: BridgeConfig): void {
    this.bridges.set(config.id, config);
  }

  public getBridge(bridgeId: string): BridgeConfig | undefined {
    return this.bridges.get(bridgeId);
  }

  public registerWrappedAsset(info: WrappedAssetInfo): void {
    this.wrappedAssets.push(info);
  }

  public getWrappedAsset(canonicalAsset: string, chainId: string): WrappedAssetInfo | undefined {
    return this.wrappedAssets.find(
      (wa) => wa.canonicalAsset === canonicalAsset && wa.chainId === chainId
    );
  }

  public getCanonicalAsset(wrappedAsset: string): WrappedAssetInfo | undefined {
    return this.wrappedAssets.find((wa) => wa.wrappedAsset === wrappedAsset);
  }

  public getSupportedBridges(chainId: string): BridgeConfig[] {
    return Array.from(this.bridges.values()).filter((bridge) =>
      bridge.supportedChains.includes(chainId) && bridge.isActive
    );
  }
}
