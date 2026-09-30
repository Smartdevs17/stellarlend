/**
 * System Domain Routes (v1)
 *
 * Aggregates all infrastructure routes under /v1/system:
 * - Health checks (liveness, readiness, coalescing, cache metrics)
 * - Configuration management
 * - Developer portal (API keys, GraphQL playground, usage, webhooks, SDK)
 * - Analytics
 * - Cache metrics, invalidation and prefetch
 */

import { Router } from 'express';
import healthRoutes from '../../health.routes';
import configRoutes from '../../config.routes';
import developerRoutes from '../../developer.routes';
import analyticsRoutes from '../../analytics.routes';
import cacheRoutes from '../../cache.routes';

const router = Router();

// Health: /v1/system/health/*
router.use('/health', healthRoutes);

// Config: /v1/system/config/*
router.use('/config', configRoutes);

// Developer portal: /v1/system/developer/*
router.use('/developer', developerRoutes);

// Analytics: /v1/system/analytics/*
router.use('/analytics', analyticsRoutes);

// Cache metrics, invalidation and prefetch: /v1/system/cache/*
router.use('/cache', cacheRoutes);

export default router;
