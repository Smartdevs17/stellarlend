import { defineConfig } from 'vitest/config';
import path from 'path';

/**
 * Oracle stress suite configuration (#691).
 *
 * Kept separate from `vitest.config.ts` so that `npm test` stays fast and the
 * stress run can be scheduled independently in CI. The default config excludes
 * `tests/stress/**` for the same reason.
 *
 * Coverage thresholds are deliberately absent: these scenarios exercise failure
 * and timing behaviour, not breadth of code, and a coverage gate here would
 * reward writing tests that touch lines rather than tests that find bugs.
 */
export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: ['tests/stress/**/*.stress.test.ts'],
    globalSetup: ['./tests/stress/globalSetup.ts'],
    // Several scenarios drive hundreds of requests through simulated outages.
    testTimeout: 60_000,
    hookTimeout: 30_000,
    reporters: ['default'],
  },
  resolve: {
    alias: {
      '@/claims': path.resolve(__dirname, './src/claims'),
      '@/config': path.resolve(__dirname, './src/config'),
      '@/devtools': path.resolve(__dirname, './src/devtools'),
      '@/providers': path.resolve(__dirname, './src/providers'),
      '@/security': path.resolve(__dirname, './src/security'),
      '@/services': path.resolve(__dirname, './src/services'),
      '@/types': path.resolve(__dirname, './src/types'),
      '@/utils': path.resolve(__dirname, './src/utils'),
    },
  },
});
