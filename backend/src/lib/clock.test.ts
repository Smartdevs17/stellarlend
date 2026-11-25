import { afterEach, describe, expect, it, vi } from 'vitest';
import { systemClock } from './clock';

describe('systemClock', () => {
  afterEach(() => vi.restoreAllMocks());

  it('exposes wall time consistently in milliseconds, seconds and Date form', () => {
    vi.spyOn(Date, 'now').mockReturnValue(1_700_000_123_456);

    expect(systemClock.nowMilliseconds()).toBe(1_700_000_123_456);
    expect(systemClock.nowSeconds()).toBe(1_700_000_123);
    expect(systemClock.nowDate().getTime()).toBe(1_700_000_123_456);
  });
});
