/**
 * Keeper submission review service (issue #1780).
 *
 * Turns an authorized reviewer's decision into an on-chain slash through an
 * injected client. Validation happens before any external call, and the
 * submission is only marked reviewed after that call succeeds, so a failed
 * slash never leaves partial state or blocks a later retry.
 */

export type ReviewDecision = 'accept' | 'slash';

export interface KeeperSubmission {
  id: string;
  keeper: string;
  payloadHash: string;
  submittedAt: number;
}

export interface SlashRequest {
  submissionId: string;
  keeper: string;
  reason: string;
}

export interface SlashOutcome {
  slashed: boolean;
  amount: number;
  txHash?: string;
}

export interface ReviewOutcome {
  submissionId: string;
  decision: ReviewDecision;
  slashed: boolean;
  amount: number;
  txHash?: string;
}

export interface SlashingClient {
  slash(request: SlashRequest): Promise<SlashOutcome>;
}

export interface ReviewLogger {
  info(event: string, fields?: Record<string, unknown>): void;
  warn(event: string, fields?: Record<string, unknown>): void;
  error(event: string, fields?: Record<string, unknown>): void;
}

export interface ReviewMetrics {
  increment(name: string, labels?: Record<string, string | number>): void;
}

export interface KeeperReviewOptions {
  reviewers: string[];
  slashClient: SlashingClient;
  logger?: ReviewLogger;
  metrics?: ReviewMetrics;
  minReasonLength?: number;
}

export type KeeperReviewErrorCode =
  | 'INVALID_SUBMISSION'
  | 'UNAUTHORIZED_REVIEWER'
  | 'INVALID_DECISION'
  | 'INVALID_REASON'
  | 'ALREADY_REVIEWED'
  | 'SLASH_FAILED'
  | 'INVALID_SLASH_OUTCOME';

export class KeeperReviewError extends Error {
  readonly code: KeeperReviewErrorCode;
  readonly details: Record<string, unknown>;

  constructor(code: KeeperReviewErrorCode, message: string, details: Record<string, unknown> = {}) {
    super(message);
    this.name = 'KeeperReviewError';
    this.code = code;
    this.details = details;
  }
}

const noopLogger: ReviewLogger = {
  info: () => {},
  warn: () => {},
  error: () => {},
};

const noopMetrics: ReviewMetrics = { increment: () => {} };

function isBlank(value: unknown): boolean {
  return typeof value !== 'string' || value.trim().length === 0;
}

export class KeeperReviewService {
  private readonly reviewers: Set<string>;
  private readonly slashClient: SlashingClient;
  private readonly logger: ReviewLogger;
  private readonly metrics: ReviewMetrics;
  private readonly minReasonLength: number;
  private readonly reviewed = new Set<string>();

  constructor(options: KeeperReviewOptions) {
    if (!options.slashClient || typeof options.slashClient.slash !== 'function') {
      throw new KeeperReviewError('INVALID_SUBMISSION', 'A slashing client with slash() is required');
    }
    if (!Array.isArray(options.reviewers) || options.reviewers.length === 0) {
      throw new KeeperReviewError('UNAUTHORIZED_REVIEWER', 'At least one reviewer must be configured');
    }
    this.reviewers = new Set(options.reviewers.map((item) => item.trim()).filter(Boolean));
    if (this.reviewers.size === 0) {
      throw new KeeperReviewError('UNAUTHORIZED_REVIEWER', 'Reviewers must not be blank');
    }
    this.slashClient = options.slashClient;
    this.logger = options.logger ?? noopLogger;
    this.metrics = options.metrics ?? noopMetrics;
    this.minReasonLength = Math.max(1, options.minReasonLength ?? 3);
  }

  isReviewed(submissionId: string): boolean {
    return this.reviewed.has(submissionId);
  }

  get reviewedCount(): number {
    return this.reviewed.size;
  }

  async review(
    submission: KeeperSubmission,
    decision: ReviewDecision,
    reviewer: string,
    reason?: string,
  ): Promise<ReviewOutcome> {
    this.validateSubmission(submission);

    if (isBlank(reviewer) || !this.reviewers.has(reviewer.trim())) {
      throw new KeeperReviewError('UNAUTHORIZED_REVIEWER', `Reviewer ${String(reviewer)} is not authorized`, {
        submissionId: submission.id,
      });
    }
    if (decision !== 'accept' && decision !== 'slash') {
      throw new KeeperReviewError('INVALID_DECISION', `Unknown review decision: ${String(decision)}`, {
        submissionId: submission.id,
      });
    }
    if (this.reviewed.has(submission.id)) {
      throw new KeeperReviewError('ALREADY_REVIEWED', `Submission ${submission.id} was already reviewed`, {
        submissionId: submission.id,
      });
    }

    if (decision === 'accept') {
      this.reviewed.add(submission.id);
      this.logger.info('keeper.review.accepted', { submissionId: submission.id, reviewer });
      this.metrics.increment('keeperReviewsTotal', { decision: 'accept' });
      return { submissionId: submission.id, decision, slashed: false, amount: 0 };
    }

    if (isBlank(reason) || (reason as string).trim().length < this.minReasonLength) {
      throw new KeeperReviewError(
        'INVALID_REASON',
        `Slash reason must be at least ${this.minReasonLength} non-blank characters`,
        { submissionId: submission.id },
      );
    }

    let outcome: SlashOutcome;
    try {
      outcome = await this.slashClient.slash({
        submissionId: submission.id,
        keeper: submission.keeper,
        reason: (reason as string).trim(),
      });
    } catch (error) {
      this.logger.error('keeper.review.slash_failed', {
        submissionId: submission.id,
        keeper: submission.keeper,
        reviewer,
        error: error instanceof Error ? error.message : String(error),
      });
      this.metrics.increment('keeperSlashFailedTotal', { submissionId: submission.id });
      throw new KeeperReviewError('SLASH_FAILED', `Slash failed for submission ${submission.id}`, {
        submissionId: submission.id,
        cause: error,
      });
    }

    if (!outcome || typeof outcome.slashed !== 'boolean' || !Number.isFinite(outcome.amount) || outcome.amount < 0) {
      this.logger.error('keeper.review.invalid_slash_outcome', { submissionId: submission.id });
      this.metrics.increment('keeperSlashFailedTotal', { submissionId: submission.id });
      throw new KeeperReviewError('INVALID_SLASH_OUTCOME', `Slashing client returned an invalid outcome for ${submission.id}`, {
        submissionId: submission.id,
      });
    }

    this.reviewed.add(submission.id);
    this.logger.info('keeper.review.slash_completed', {
      submissionId: submission.id,
      keeper: submission.keeper,
      reviewer,
      amount: outcome.amount,
      txHash: outcome.txHash,
    });
    this.metrics.increment('keeperSlashTotal', { submissionId: submission.id });
    return {
      submissionId: submission.id,
      decision,
      slashed: outcome.slashed,
      amount: outcome.amount,
      txHash: outcome.txHash,
    };
  }

  private validateSubmission(submission: KeeperSubmission): void {
    if (!submission || isBlank(submission.id) || isBlank(submission.keeper) || isBlank(submission.payloadHash)
      || !Number.isFinite(submission.submittedAt) || submission.submittedAt <= 0) {
      throw new KeeperReviewError('INVALID_SUBMISSION', 'Submission must have an id, keeper, payload hash and positive timestamp');
    }
  }
}

export function createKeeperReviewService(options: KeeperReviewOptions): KeeperReviewService {
  return new KeeperReviewService(options);
}
