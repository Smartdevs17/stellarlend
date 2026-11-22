import type { HealthStatus, RawPriceData } from '../types/index.js';

/** Transport-independent contract for live and cached price adapters. */
export interface PriceProvider {
  readonly name: string;
  readonly priority: number;
  readonly weight: number;
  readonly isEnabled: boolean;
  fetchPrice(asset: string): Promise<RawPriceData>;
  fetchPrices(assets: string[]): Promise<RawPriceData[]>;
  healthCheck(): Promise<HealthStatus>;
}
