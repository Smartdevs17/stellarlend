/** Shown when an error does not match any known case. */
export const DEFAULT_ERROR_MESSAGE = 'Something went wrong. Please try again.';

const FRIENDLY_MESSAGES: Array<{ pattern: RegExp; message: string }> = [
  {
    pattern: /user (declined|rejected|denied)|rejected by (the )?user|request rejected/i,
    message: 'The request was cancelled in your wallet.',
  },
  {
    pattern: /timed? ?out/i,
    message: 'The network took too long to respond. Please try again.',
  },
  {
    pattern: /failed to fetch|network ?error|offline|ECONNREFUSED|ERR_NETWORK/i,
    message: "We couldn't reach StellarLend. Check your connection and try again.",
  },
  {
    pattern: /\b429\b|too many requests|rate limit/i,
    message: 'Too many requests right now. Please wait a moment and try again.',
  },
  {
    pattern: /insufficient (balance|funds)|underfunded/i,
    message: 'Your balance is too low for this transaction, including network fees.',
  },
  {
    pattern: /health factor|undercollateral|insufficient collateral/i,
    message: 'This would put your position at risk of liquidation. Add collateral or use a smaller amount.',
  },
  {
    pattern: /paused/i,
    message: 'Lending is temporarily paused. Your funds are safe — please try again later.',
  },
];

/**
 * Maps an error to a message suitable for end users, hiding raw technical
 * details such as stack traces and RPC error codes. Unknown errors get a
 * generic message.
 */
export function getFriendlyErrorMessage(error: unknown): string {
  const raw = error instanceof Error ? error.message : typeof error === 'string' ? error : '';
  const match = FRIENDLY_MESSAGES.find(({ pattern }) => pattern.test(raw));
  return match ? match.message : DEFAULT_ERROR_MESSAGE;
}
