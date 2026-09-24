import type { CSSProperties } from 'react';

/** Minimum size (px) of a control so it is easy to tap on a phone. */
export const TOUCH_TARGET_SIZE = 44;

/**
 * Grid that fits as many columns of at least `minColumnWidth` px as the
 * container allows and collapses to a single column on narrow screens —
 * responsive without media queries, so it works with inline styles.
 */
export function responsiveGrid(minColumnWidth: number, gap = 12): CSSProperties {
  return {
    display: 'grid',
    gridTemplateColumns: `repeat(auto-fit, minmax(min(100%, ${minColumnWidth}px), 1fr))`,
    gap,
  };
}

/**
 * Base style for card action buttons: tall enough to tap, and wrapping two
 * per row instead of squeezing four into a phone-width card.
 */
export const actionButtonBase: CSSProperties = {
  flex: '1 1 120px',
  minHeight: TOUCH_TARGET_SIZE,
  padding: '8px 12px',
  borderRadius: 8,
  cursor: 'pointer',
  fontSize: 13,
};
