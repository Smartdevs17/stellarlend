/**
 * Example: turning contract failures into UI messages and retry decisions.
 *
 * Every `StellarLendClient` call rejects with a `ProtocolError`. Branch on `code`, show
 * your own copy for the cases users can fix, and retry only when `retryable` is true.
 */
import {
  ProtocolError,
  ProtocolErrorCode,
  normalizeProtocolError,
  protocolErrorToJson,
  type BorrowParams,
  type StellarLendClient,
} from '../src';

export function messageForUser(error: ProtocolError): string {
  switch (error.code) {
    case ProtocolErrorCode.InsufficientCollateral:
      return 'Add more collateral: borrows need at least 150% collateral.';
    case ProtocolErrorCode.LimitExceeded:
      return 'The pool is at its borrowing limit right now. Try a smaller amount or try later.';
    case ProtocolErrorCode.Unauthorized:
      return 'Your wallet did not authorize this transaction.';
    case ProtocolErrorCode.InvalidAmount:
      return 'Enter an amount greater than zero.';
    default:
      return error.message;
  }
}

/**
 * Raw `Error(Contract, #n)` code from the original failure, if any. Useful for logging:
 * contract-local codes (e.g. lending's `UtilizationThrottled = 13`) are kept in `details`.
 */
export function contractErrorCode(error: ProtocolError): number | undefined {
  const raw = error.details instanceof Error ? error.details.message : String(error.details ?? '');
  const match = /Error\(Contract,\s*#(\d+)\)/.exec(raw);
  return match ? Number(match[1]) : undefined;
}

export type BorrowOutcome =
  | { ok: true; attempts: number }
  | { ok: false; attempts: number; userMessage: string; error: ReturnType<typeof protocolErrorToJson> };

export async function borrowWithRetry(
  client: StellarLendClient,
  params: BorrowParams,
  options: { attempts?: number; backoffMs?: number; sleep?: (ms: number) => Promise<void> } = {},
): Promise<BorrowOutcome> {
  const attempts = options.attempts ?? 3;
  const backoffMs = options.backoffMs ?? 1_000;
  const sleep = options.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));

  for (let attempt = 1; ; attempt++) {
    try {
      await client.borrow(params);
      return { ok: true, attempts: attempt };
    } catch (thrown) {
      const error = normalizeProtocolError(thrown);
      if (!error.retryable || attempt >= attempts) {
        return { ok: false, attempts: attempt, userMessage: messageForUser(error), error: protocolErrorToJson(error) };
      }
      await sleep(backoffMs * attempt); // linear backoff
    }
  }
}
