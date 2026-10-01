/**
 * Price Validator Service
 *
 * Validates and sanitizes price data before it's used for
 * contract updates. Implements multiple validation checks:
 */
import { scalePrice } from '../config.js';
import { logger } from '../utils/logger.js';
/**
 * Default validator configuration
 */
const DEFAULT_CONFIG = {
    maxDeviationPercent: 10,
    maxStalenessSeconds: 300,
    minPrice: 0.0000001,
    maxPrice: 1000000000,
    sourceWeights: {
        coingecko: 1.0,
        binance: 0.95,
        coinmarketcap: 1.0,
    },
    twapDeviationPercent: 5,
    rateManipulationPercent: 10,
    manipulationSequenceLength: 3,
    maxHistorySamples: 120,
};
/**
 * Price Validator
 */
export class PriceValidator {
    config;
    cachedPrices = new Map();
    priceHistory = new Map();
    constructor(config = {}) {
        this.config = { ...DEFAULT_CONFIG, ...config };
        logger.info('Price validator initialized', {
            maxDeviationPercent: this.config.maxDeviationPercent,
            maxStalenessSeconds: this.config.maxStalenessSeconds,
        });
    }
    /**
     * Validate raw price data and convert to validated PriceData
     */
    validate(raw) {
        const errors = [];
        if (raw.price <= 0) {
            errors.push({
                code: 'PRICE_ZERO',
                message: `Price must be positive, got ${raw.price}`,
            });
        }
        if (raw.price < this.config.minPrice) {
            errors.push({
                code: 'PRICE_ZERO',
                message: `Price ${raw.price} below minimum ${this.config.minPrice}`,
            });
        }
        if (raw.price > this.config.maxPrice) {
            errors.push({
                code: 'PRICE_DEVIATION_TOO_HIGH',
                message: `Price ${raw.price} exceeds maximum ${this.config.maxPrice}`,
            });
        }
        const now = Math.floor(Date.now() / 1000);
        const age = now - raw.timestamp;
        if (age > this.config.maxStalenessSeconds) {
            errors.push({
                code: 'PRICE_STALE',
                message: `Price is ${age}s old, max allowed is ${this.config.maxStalenessSeconds}s`,
                details: { age, maxAge: this.config.maxStalenessSeconds },
            });
        }
        const cachedPrice = this.cachedPrices.get(raw.asset);
        if (cachedPrice !== undefined) {
            const deviation = Math.abs((raw.price - cachedPrice) / cachedPrice) * 100;
            if (deviation > this.config.maxDeviationPercent) {
                errors.push({
                    code: 'PRICE_DEVIATION_TOO_HIGH',
                    message: `Price deviation ${deviation.toFixed(2)}% exceeds max ${this.config.maxDeviationPercent}%`,
                    details: {
                        newPrice: raw.price,
                        cachedPrice,
                        deviationPercent: deviation,
                    },
                });
            }
        }
        // Manipulation detection: watch for a run of large, direction-consistent
        // rate moves that exceed the manipulation threshold (issue #847). Mirrors
        // the on-chain rate guard's per-block deviation detection and attempts log.
        if (cachedPrice !== undefined && errors.length === 0) {
            const manipulation = this.detectManipulation(raw, cachedPrice);
            if (manipulation) {
                errors.push(manipulation);
            }
        }
        if (errors.length === 0) {
            const validatedPrice = {
                asset: raw.asset.toUpperCase(),
                price: scalePrice(raw.price),
                timestamp: raw.timestamp,
                source: raw.source,
                confidence: this.calculateConfidence(raw, cachedPrice),
            };
            this.cachedPrices.set(raw.asset, raw.price);
            this.recordSample(raw.asset, raw.price);
            return {
                isValid: true,
                price: validatedPrice,
                errors: [],
            };
        }
        logger.warn(`Price validation failed for ${raw.asset}`, { errors });
        return {
            isValid: false,
            errors,
        };
    }
    validateWithTwap(raw, twapPrice) {
        const result = this.validate(raw);
        if (!result.isValid || twapPrice === undefined || twapPrice <= 0) {
            return result;
        }
        const twapThreshold = this.config.twapDeviationPercent ?? 5;
        const deviation = Math.abs((raw.price - twapPrice) / twapPrice) * 100;
        if (deviation > twapThreshold) {
            result.isValid = false;
            result.errors.push({
                code: 'PRICE_DEVIATION_TOO_HIGH',
                message: `Spot price deviates ${deviation.toFixed(2)}% from TWAP (max ${twapThreshold}%)`,
                details: { spotPrice: raw.price, twapPrice, deviationPercent: deviation },
            });
        }
        return result;
    }
    validateRateChange(oldRate, newRate) {
        const errors = [];
        const threshold = this.config.rateManipulationPercent ?? 10;
        if (oldRate > 0) {
            const change = Math.abs((newRate - oldRate) / oldRate) * 100;
            if (change > threshold) {
                errors.push({
                    code: 'PRICE_DEVIATION_TOO_HIGH',
                    message: `Rate change ${change.toFixed(2)}% exceeds ${threshold}% manipulation threshold`,
                    details: { oldRate, newRate, changePercent: change },
                });
            }
        }
        return errors.length === 0 ? { isValid: true, errors: [] } : { isValid: false, errors };
    }
    validateMany(prices) {
        return prices.map((p) => this.validate(p));
    }
    /**
     * Detect rate manipulation by looking for a sustained, direction-consistent
     * run of price moves that each exceed the manipulation threshold (issue #847).
     *
     * Mirrors the on-chain rate guard which flags attempts whenever the per-block
     * rate deviation is unusually large, and pauses once too many are logged.
     */
    detectManipulation(raw, previousPrice) {
        const threshold = this.config.rateManipulationPercent ?? 10;
        if (previousPrice <= 0) {
            return null;
        }
        const step = (raw.price - previousPrice) / previousPrice;
        if (Math.abs(step) <= threshold / 100) {
            return null;
        }
        const direction = step > 0 ? 1 : -1;
        // Walk the rolling history (newest last) backwards, counting consecutive
        // price moves in the same direction that each exceed the threshold. The
        // history's newest element equals `previousPrice`; the current `step` is
        // the first move in the run.
        const history = this.priceHistory.get(raw.asset.toUpperCase()) ?? [];
        let run = 1;
        let prior = previousPrice;
        for (let i = history.length - 1; i >= 0; i -= 1) {
            const nextPrior = history[i];
            if (nextPrior <= 0)
                break;
            const priorStep = (prior - nextPrior) / nextPrior;
            const priorDir = priorStep > 0 ? 1 : -1;
            if (priorDir === direction && Math.abs(priorStep) > threshold / 100) {
                run += 1;
                prior = nextPrior;
            }
            else {
                break;
            }
        }
        const seqLen = this.config.manipulationSequenceLength ?? 3;
        if (run >= seqLen) {
            return {
                code: 'RATE_MANIPULATION_DETECTED',
                message: `Suspected rate manipulation: ${run} consecutive ${direction > 0 ? 'upward' : 'downward'} moves each exceeding ${threshold}%`,
                details: {
                    priorPrice: previousPrice,
                    newPrice: raw.price,
                    consecutiveMoves: run,
                    stepPercent: Math.abs(step) * 100,
                },
            };
        }
        return null;
    }
    /**
     * Record a validated price sample for manipulation scoring, keeping a bounded
     * history for each asset.
     */
    recordSample(asset, price) {
        const key = asset.toUpperCase();
        const history = this.priceHistory.get(key) ?? [];
        history.push(price);
        const max = this.config.maxHistorySamples ?? 120;
        while (history.length > max) {
            history.shift();
        }
        this.priceHistory.set(key, history);
    }
    /**
     * Return the retained rolling price history for an asset (newest last).
     * Useful for off-chain manipulation audits and dashboards.
     */
    getPriceHistory(asset) {
        return this.priceHistory.get(asset.toUpperCase()) ?? [];
    }
    /**
     * Clear the retained price history for an asset (or all assets).
     */
    clearPriceHistory(asset) {
        if (asset) {
            this.priceHistory.delete(asset.toUpperCase());
        }
        else {
            this.priceHistory.clear();
        }
    }
    /**
     * Calculate confidence score based on various factors
     */
    calculateConfidence(raw, cachedPrice) {
        let confidence = 100;
        const now = Math.floor(Date.now() / 1000);
        const age = now - raw.timestamp;
        const ageRatio = age / this.config.maxStalenessSeconds;
        confidence -= Math.min(20, ageRatio * 20);
        if (cachedPrice !== undefined) {
            const deviation = Math.abs((raw.price - cachedPrice) / cachedPrice) * 100;
            const deviationRatio = deviation / this.config.maxDeviationPercent;
            confidence -= Math.min(30, deviationRatio * 30);
        }
        // Apply configurable source weight
        const sourceWeight = this.config.sourceWeights[raw.source] || 1.0;
        confidence *= sourceWeight;
        return Math.max(0, Math.min(100, confidence));
    }
    /**
     * Update cached price manually (e.g., after successful contract update)
     */
    updateCache(asset, price) {
        this.cachedPrices.set(asset.toUpperCase(), price);
    }
    /**
     * Clear cached price for an asset
     */
    clearCache(asset) {
        if (asset) {
            this.cachedPrices.delete(asset.toUpperCase());
        }
        else {
            this.cachedPrices.clear();
        }
    }
    /**
     * Get current cache state (for debugging)
     */
    getCacheState() {
        return Object.fromEntries(this.cachedPrices);
    }
    /**
     * Maximum tolerated deviation, in percent.
     *
     * Exposed so the aggregator can apply the same threshold when screening a
     * round of quotes against their consensus, before any of them is allowed to
     * become this validator's drift reference.
     */
    get maxDeviationPercent() {
        return this.config.maxDeviationPercent;
    }
}
/**
 * Create a validator with custom configuration
 */
export function createValidator(config) {
    return new PriceValidator(config);
}
//# sourceMappingURL=price-validator.js.map