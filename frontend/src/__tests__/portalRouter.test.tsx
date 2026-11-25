import { describe, it, expect } from 'vitest';
import { RouteLoadingFallback, DeveloperPortalRouter } from '../portalRouter';

describe('DeveloperPortalRouter', () => {
  it('exports valid component definitions', () => {
    expect(typeof RouteLoadingFallback).toBe('function');
    expect(typeof DeveloperPortalRouter).toBe('function');
  });
});
