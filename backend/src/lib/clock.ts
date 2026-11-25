/** Shared wall-clock abstraction. All backend wall-clock values use Unix time. */
export interface Clock {
  /** Current time in epoch milliseconds. */
  nowMilliseconds(): number;
  /** Current time in epoch seconds. */
  nowSeconds(): number;
  /** Current time as a fresh Date instance. */
  nowDate(): Date;
}

const nowMilliseconds = (): number => Date.now();

/** Production clock; services can accept a Clock to make time deterministic. */
export const systemClock: Clock = Object.freeze({
  nowMilliseconds,
  nowSeconds: () => Math.floor(nowMilliseconds() / 1000),
  nowDate: () => new Date(nowMilliseconds()),
});
