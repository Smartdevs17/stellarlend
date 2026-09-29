import { StellarAssetService } from './services/StellarAssetService';
import { BridgeService } from './bridge/BridgeService';
import { BridgeRegistry } from './bridge/BridgeRegistry';
import { BRIDGE_CONFIGS, WRAPPED_ASSETS } from './config/bridgeConfig';

// Initialize bridge registry with configurations
const registry = BridgeRegistry.getInstance();
BRIDGE_CONFIGS.forEach((config) => registry.registerBridge(config));
WRAPPED_ASSETS.forEach((asset) => registry.registerWrappedAsset(asset));

// Initialize services
const stellarService = new StellarAssetService();
const bridgeService = new BridgeService(stellarService);

export { BridgeService, BridgeRegistry, stellarService, bridgeService };
export type {
  CrossChainTransferRequest,
  CrossChainTransferResponse,
} from './bridge/BridgeService';
export type { BridgeConfig, WrappedAssetInfo } from './bridge/BridgeRegistry';
