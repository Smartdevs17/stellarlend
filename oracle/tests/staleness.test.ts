/**
 * Tests for Oracle Price Staleness Detection
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { OracleService } from '../src/index.js';
import { logger, logStalenessAlert } from '../src/utils/logger.js';

// Mock logger to verify calls
vi.mock('../src/utils/logger.js', async () => {
    const actual = await vi.importActual('../src/utils/logger.js');
    return {
        ...actual,
        logger: {
            info: vi.fn(),
            warn: vi.fn(),
            error: vi.fn(),
            debug: vi.fn(),
        },
        logStalenessAlert: vi.fn(),
    };
});

// Mock contract updater
vi.mock('../src/services/contract-updater.js', () => ({
    createContractUpdater: vi.fn(() => ({
        updatePrices: vi.fn().mockResolvedValue([{ success: true, asset: 'XLM', price: 100n, timestamp: Date.now() }]),
        healthCheck: vi.fn().mockResolvedValue(true),
        getAdminPublicKey: vi.fn().mockReturnValue('GTEST123'),
    })),
    ContractUpdater: vi.fn(),
}));

// Mock aggregator
vi.mock('../src/services/price-aggregator.js', () => ({
    createAggregator: vi.fn(() => ({
        getPrices: vi.fn().mockResolvedValue(new Map([['XLM', { asset: 'XLM', price: 100n, timestamp: Date.now() }]])),
        getPrice: vi.fn(),
        getProviders: vi.fn().mockReturnValue([]),
        getStats: vi.fn().mockReturnValue({}),
    })),
}));

describe('Oracle Price Staleness Detection', () => {
    let service: OracleService;
    const STALE_THRESHOLD = 300; // 5 minutes

    const mockConfig: any = {
        stellarNetwork: 'testnet',
        stellarRpcUrl: 'http://localhost:8000',
        contractId: 'CTEST123',
        adminSecretKey: 'S123',
        updateIntervalMs: 60000,
        maxPriceDeviationPercent: 10,
        priceStaleThresholdSeconds: STALE_THRESHOLD,
        cacheTtlSeconds: 30,
        logLevel: 'info',
        providers: [],
    };

    beforeEach(() => {
        vi.useFakeTimers();
        vi.setSystemTime(new Date('2026-03-24T12:00:00Z'));
        service = new OracleService(mockConfig);
    });

    afterEach(() => {
        vi.useRealTimers();
        vi.restoreAllMocks();
    });

    it('should not log staleness alert on first update', async () => {
        await service.updatePrices(['XLM']);
        expect(logStalenessAlert).not.toHaveBeenCalled();
    });

    it('should not log staleness alert if update happens within threshold', async () => {
        // First successful update
        await service.updatePrices(['XLM']);

        // Advance time by 4 minutes (less than 5m threshold)
        vi.advanceTimersByTime(4 * 60 * 1000);

        await service.updatePrices(['XLM']);
        expect(logStalenessAlert).not.toHaveBeenCalled();
    });

    it('should log staleness alert if update age exceeds threshold', async () => {
        // First successful update
        await service.updatePrices(['XLM']);

        // Advance time by 6 minutes (more than 5m threshold)
        vi.advanceTimersByTime(6 * 60 * 1000);

        await service.updatePrices(['XLM']);

        expect(logStalenessAlert).toHaveBeenCalledWith(
            expect.any(Number), // ageSeconds around 360
            STALE_THRESHOLD,
            expect.any(Number) // lastUpdateTime
        );

        const callArgs = vi.mocked(logStalenessAlert).mock.calls[0];
        expect(callArgs[0]).toBe(360); // 6 minutes in seconds
    });

    it('should update lastSuccessfulUpdate after a successful cycle', async () => {
        // First update
        await service.updatePrices(['XLM']);

        // Advance time by 4 minutes
        vi.advanceTimersByTime(4 * 60 * 1000);
        await service.updatePrices(['XLM']);

        // Advance another 4 minutes (total 8 from start, but only 4 from last update)
        vi.advanceTimersByTime(4 * 60 * 1000);
        await service.updatePrices(['XLM']);

        expect(logStalenessAlert).not.toHaveBeenCalled();
    });

    it('should log alert even if price fetching fails but cycle starts', async () => {
        // First success
        await service.updatePrices(['XLM']);

        // Advance beyond threshold
        vi.advanceTimersByTime(6 * 60 * 1000);

        // Mock failure for the NEXT getPrices call
        const { createAggregator } = await import('../src/services/price-aggregator.js');
        vi.mocked(createAggregator().getPrices).mockRejectedValueOnce(new Error('API Down'));

        await service.updatePrices(['XLM']);

        // Alert should still be triggered because it's checked at the start of the cycle
        expect(logStalenessAlert).toHaveBeenCalled();
    });
});
