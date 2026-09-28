import { Config } from './base';

interface StablecoinConfig {
  targetPegPrice: number; // e.g., 1.0 for USDT/USDC
  thresholdPercentage: number; // % deviation before action
  maxTransferAmount: number; // USD value per transfer
  minLiquidityRatio: number; // Minimum liquidity ratio to maintain
  checkIntervalMinutes: number;
  pools: string[]; // Whitelisted stablecoin pool IDs
}

const stablecoinConfig: StablecoinConfig = {
  targetPegPrice: 1.0,
  thresholdPercentage: 0.01, // 1% deviation
  maxTransferAmount: 1000000, // $1M per transfer
  minLiquidityRatio: 0.95,
  checkIntervalMinutes: 15,
  pools: ['USDT_POOL_1', 'USDC_POOL_2', 'DAI_POOL_3']
};

export { StablecoinConfig, stablecoinConfig };
