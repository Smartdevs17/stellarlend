/**
 * Price Validator Service
 *
 * Validates and sanitizes price data before it's used for
 * contract updates. Implements multiple validation checks:
 */
import type { RawPriceData, ValidationResult } from '../types/index.js';
/**
 * Validator configuration
 */
export interface ValidatorConfig {
    maxDeviationPercent: number;
    maxStalenessSeconds: number;
    minPrice: number;
    maxPrice: number;
    sourceWeights: Record<string, number>;
    /** TWAP deviation threshold for manipulation detection (default 5%). */
    twapDeviationPercent?: number;
    /** Rate change threshold for manipulation alerts (default 10%). */
    rateManipulationPercent?: number;
    /** Consecutive direction-consistent moves that trip the manipulation guard (default 3). */
    manipulationSequenceLength?: number;
    /** Max number of price samples retained per asset for manipulation scoring. */
    maxHistorySamples?: number;
}
/**
 * Price Validator
 */
export declare class PriceValidator {
    private config;
    private cachedPrices;
    private readonly priceHistory;
    constructor(config?: Partial<ValidatorConfig>);
    /**
     * Validate raw price data and convert to validated PriceData
     */
    validate(raw: RawPriceData): ValidationResult;
    validateWithTwap(raw: RawPriceData, twapPrice?: number): ValidationResult;
    validateRateChange(oldRate: number, newRate: number): ValidationResult;
    validateMany(prices: RawPriceData[]): ValidationResult[];
    /**
     * Detect rate manipulation by looking for a sustained, direction-consistent
     * run of price moves that each exceed the manipulation threshold (issue #847).
     *
     * Mirrors the on-chain rate guard which flags attempts whenever the per-block
     * rate deviation is unusually large, and pauses once too many are logged.
     */
    private detectManipulation;
    /**
     * Record a validated price sample for manipulation scoring, keeping a bounded
     * history for each asset.
     */
    private recordSample;
    /**
     * Return the retained rolling price history for an asset (newest last).
     * Useful for off-chain manipulation audits and dashboards.
     */
    getPriceHistory(asset: string): number[];
    /**
     * Clear the retained price history for an asset (or all assets).
     */
    clearPriceHistory(asset?: string): void;
    /**
     * Calculate confidence score based on various factors
     */
    private calculateConfidence;
    /**
     * Update cached price manually (e.g., after successful contract update)
     */
    updateCache(asset: string, price: number): void;
    /**
     * Clear cached price for an asset
     */
    clearCache(asset?: string): void;
    /**
     * Get current cache state (for debugging)
     */
    getCacheState(): Record<string, number>;
    /**
     * Maximum tolerated deviation, in percent.
     *
     * Exposed so the aggregator can apply the same threshold when screening a
     * round of quotes against their consensus, before any of them is allowed to
     * become this validator's drift reference.
     */
    get maxDeviationPercent(): number;
}
/**
 * Create a validator with custom configuration
 */
export declare function createValidator(config?: Partial<ValidatorConfig>): PriceValidator;
//# sourceMappingURL=price-validator.d.ts.map