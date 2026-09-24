/**
 * Governance Parameter Store Service — Issue #697
 *
 * Mirrors the on-chain `parameter-store` contract
 * (`stellar-lend/contracts/parameter-store`) so the API can serve proposals,
 * votes, versions and impact simulations without a round trip per read.
 *
 * The projections in `simulateChange` are a faithful port of
 * `parameter-store/src/simulation.rs`: the same formulas, thresholds and
 * warning names, so a voter reading the API and a voter calling the contract
 * see the same numbers. When one changes, the other must change with it.
 *
 * Like the other services in this API, state is held in memory pending the
 * contract-client wiring; the shapes are the contract's.
 */

import logger from '../../utils/logger';
import { ValidationError, NotFoundError } from '../../utils/errors';

export const BPS_DIVISOR = 10_000;
export const RISK_TIMELOCK_SECONDS = 48 * 3600;
export const STANDARD_TIMELOCK_SECONDS = 24 * 3600;
export const EMERGENCY_TIMELOCK_SECONDS = 4 * 3600;

export type ParameterType =
  | 'LTV'
  | 'LiquidationThreshold'
  | 'CloseFactor'
  | 'LiquidationIncentive'
  | 'ReserveFactor'
  | 'DebtCeiling'
  | 'BaseInterestRate'
  | 'Slope1'
  | 'Slope2'
  | 'OptimalUtilization';

export const PARAMETER_TYPES: ParameterType[] = [
  'LTV',
  'LiquidationThreshold',
  'CloseFactor',
  'LiquidationIncentive',
  'ReserveFactor',
  'DebtCeiling',
  'BaseInterestRate',
  'Slope1',
  'Slope2',
  'OptimalUtilization',
];

/** Parameters whose change can put existing positions at risk. */
const RISK_PARAMETERS: ParameterType[] = [
  'LTV',
  'LiquidationThreshold',
  'CloseFactor',
  'LiquidationIncentive',
];

/** Valid range per parameter, in basis points unless noted. */
const RANGES: Record<ParameterType, { min: number; max: number; exclusive?: boolean }> = {
  LTV: { min: 1, max: 9_000 },
  LiquidationThreshold: { min: 1, max: BPS_DIVISOR },
  CloseFactor: { min: 1, max: BPS_DIVISOR },
  LiquidationIncentive: { min: 1_000, max: 2_000 },
  ReserveFactor: { min: 0, max: BPS_DIVISOR },
  DebtCeiling: { min: 0, max: Number.MAX_SAFE_INTEGER },
  BaseInterestRate: { min: 0, max: 5_000 },
  Slope1: { min: 0, max: BPS_DIVISOR },
  Slope2: { min: 0, max: 50_000 },
  OptimalUtilization: { min: 1, max: BPS_DIVISOR - 1 },
};

export type ImpactSeverity = 'negligible' | 'low' | 'moderate' | 'high';

export interface PoolSnapshot {
  totalCollateral: number;
  totalDebt: number;
  totalDeposits: number;
  atRiskDebt: number;
  atRiskBandBps: number;
}

export interface ParameterImpact {
  parameter: ParameterType;
  currentValue: number;
  proposedValue: number;
  relativeChangeBps: number;
  borrowingPowerDelta: number;
  newlyLiquidatableDebt: number;
  borrowRateDeltaBps: number;
  severity: ImpactSeverity;
  warnings: string[];
}

export interface ParameterProposal {
  id: number;
  pool: string;
  parameter: ParameterType;
  proposedValue: number;
  proposer: string;
  createdAt: number;
  effectiveAt: number;
  accepted: boolean;
  rejected: boolean;
  isEmergency: boolean;
}

export interface ParameterChange {
  parameter: ParameterType;
  oldValue: number;
  newValue: number;
  timestamp: number;
  effectiveAt: number;
  changedBy: string;
  version: number;
}

export interface VotingConfig {
  quorumBps: number;
  approvalThresholdBps: number;
  votingPeriodSeconds: number;
}

export interface ParameterVote {
  proposalId: number;
  voter: string;
  support: boolean;
  weight: number;
  votedAt: number;
}

export interface VoteTally {
  proposalId: number;
  forWeight: number;
  againstWeight: number;
  voterCount: number;
  totalVotingPower: number;
  participation: number;
  meetsQuorum: boolean;
  isApproved: boolean;
  hasPassed: boolean;
}

export interface ParameterChangeNotification {
  pool: string;
  parameter: ParameterType;
  oldValue: number;
  newValue: number;
  version: number;
  effectiveAt: number;
  isEmergency: boolean;
  emittedAt: number;
}

/** Key for the per-pool parameter maps. */
function key(pool: string, parameter: ParameterType): string {
  return `${pool}::${parameter}`;
}

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

/** Truncating integer division, matching the contract's `i128` arithmetic. */
function intDiv(a: number, b: number): number {
  if (b === 0) return 0;
  return Math.trunc(a / b);
}

function applyBps(amount: number, bps: number): number {
  return intDiv(amount * bps, BPS_DIVISOR);
}

function ratioBps(numerator: number, denominator: number): number {
  if (denominator === 0) return 0;
  return intDiv(numerator * BPS_DIVISOR, denominator);
}

/** The kinked borrow-rate curve, mirroring `stellarlend_math::rates::kink_rate`. */
function kinkRate(
  utilizationBps: number,
  base: number,
  kink: number,
  slope1: number,
  slope2: number,
): number {
  if (utilizationBps <= kink) {
    if (kink === 0) return base;
    return base + intDiv(utilizationBps * slope1, kink);
  }
  const rateAtKink = base + slope1;
  const maxExcess = BPS_DIVISOR - kink;
  if (maxExcess === 0) return rateAtKink;
  return rateAtKink + intDiv((utilizationBps - kink) * slope2, maxExcess);
}

export function isRiskParameter(parameter: ParameterType): boolean {
  return RISK_PARAMETERS.includes(parameter);
}

export function minTimelockFor(parameter: ParameterType): number {
  return isRiskParameter(parameter) ? RISK_TIMELOCK_SECONDS : STANDARD_TIMELOCK_SECONDS;
}

export function validateRange(parameter: ParameterType, value: number): boolean {
  const range = RANGES[parameter];
  if (!range) return false;
  return Number.isFinite(value) && value >= range.min && value <= range.max;
}

export class ParametersService {
  private pools = new Set<string>();
  private values = new Map<string, number>();
  private versions = new Map<string, number>();
  private versionedValues = new Map<string, number>();
  private history = new Map<string, ParameterChange[]>();
  private proposals = new Map<number, ParameterProposal>();
  private votes = new Map<number, ParameterVote[]>();
  private votingPower = new Map<string, number>();
  private votingConfig: VotingConfig | null = null;
  private notifications: ParameterChangeNotification[] = [];
  private proposalCounter = 0;
  private emergencyActive = false;

  // ------------------------------------------------------------------ pools

  registerPool(pool: string): void {
    if (!pool) throw new ValidationError('pool is required');
    this.pools.add(pool);
  }

  listPools(): string[] {
    return [...this.pools];
  }

  private requirePool(pool: string): void {
    if (!this.pools.has(pool)) {
      throw new NotFoundError(`Pool not registered: ${pool}`);
    }
  }

  // ------------------------------------------------------------- parameters

  getParameter(pool: string, parameter: ParameterType): number {
    return this.values.get(key(pool, parameter)) ?? 0;
  }

  /** Every parameter set for a pool, defaulting unset ones to 0. */
  getPoolParameters(pool: string): Record<ParameterType, number> {
    const result = {} as Record<ParameterType, number>;
    for (const parameter of PARAMETER_TYPES) {
      result[parameter] = this.getParameter(pool, parameter);
    }
    return result;
  }

  getVersion(pool: string, parameter: ParameterType): number {
    return this.versions.get(key(pool, parameter)) ?? 0;
  }

  /**
   * The value a parameter held at a given version.
   *
   * @throws NotFoundError for a version that was never minted, rather than
   *   returning a default a caller could mistake for a historical value.
   */
  getParameterAtVersion(pool: string, parameter: ParameterType, version: number): number {
    const value = this.versionedValues.get(`${key(pool, parameter)}::${version}`);
    if (value === undefined) {
      throw new NotFoundError(
        `No version ${version} recorded for ${parameter} on pool ${pool}`,
      );
    }
    return value;
  }

  getHistory(pool: string, parameter: ParameterType): ParameterChange[] {
    return [...(this.history.get(key(pool, parameter)) ?? [])];
  }

  /**
   * Validates a value against its own range **and** against the other
   * parameters already set for the pool.
   *
   * Range checks alone cannot catch an LTV of 80% against a liquidation
   * threshold of 75%: both are individually legal, together they let a borrower
   * open a position that is immediately liquidatable.
   */
  validateValue(
    pool: string,
    parameter: ParameterType,
    value: number,
  ): { valid: boolean; reason?: string } {
    if (!validateRange(parameter, value)) {
      const range = RANGES[parameter];
      return { valid: false, reason: `${parameter} must be between ${range.min} and ${range.max}` };
    }

    const ltv = this.getParameter(pool, 'LTV');
    const threshold = this.getParameter(pool, 'LiquidationThreshold');

    if (parameter === 'LTV' && threshold > 0 && value >= threshold) {
      return {
        valid: false,
        reason: `LTV (${value}) must stay below the liquidation threshold (${threshold})`,
      };
    }
    if (parameter === 'LiquidationThreshold' && ltv > 0 && value <= ltv) {
      return {
        valid: false,
        reason: `Liquidation threshold (${value}) must stay above LTV (${ltv})`,
      };
    }
    return { valid: true };
  }

  // -------------------------------------------------------------- proposals

  proposeChange(params: {
    pool: string;
    parameter: ParameterType;
    value: number;
    proposer: string;
    timelockSeconds?: number;
  }): ParameterProposal {
    const { pool, parameter, value, proposer } = params;
    this.requirePool(pool);

    const validation = this.validateValue(pool, parameter, value);
    if (!validation.valid) {
      throw new ValidationError(validation.reason ?? 'Invalid parameter value');
    }

    const minTimelock = minTimelockFor(parameter);
    const timelock = params.timelockSeconds ?? minTimelock;
    if (timelock < minTimelock) {
      throw new ValidationError(
        `Timelock too short for ${parameter}: minimum ${minTimelock}s`,
      );
    }

    const createdAt = nowSeconds();
    this.proposalCounter += 1;
    const proposal: ParameterProposal = {
      id: this.proposalCounter,
      pool,
      parameter,
      proposedValue: value,
      proposer,
      createdAt,
      effectiveAt: createdAt + timelock,
      accepted: false,
      rejected: false,
      isEmergency: false,
    };
    this.proposals.set(proposal.id, proposal);
    logger.info('Parameter change proposed', { id: proposal.id, pool, parameter, value });
    return proposal;
  }

  proposeEmergencyChange(params: {
    pool: string;
    parameter: ParameterType;
    value: number;
    proposer: string;
  }): ParameterProposal {
    const { pool, parameter, value, proposer } = params;
    this.requirePool(pool);
    if (!isRiskParameter(parameter)) {
      throw new ValidationError('Emergency override is only available for risk parameters');
    }
    if (!validateRange(parameter, value)) {
      throw new ValidationError(`${parameter} value out of range`);
    }

    const createdAt = nowSeconds();
    this.proposalCounter += 1;
    const proposal: ParameterProposal = {
      id: this.proposalCounter,
      pool,
      parameter,
      proposedValue: value,
      proposer,
      createdAt,
      effectiveAt: createdAt + EMERGENCY_TIMELOCK_SECONDS,
      accepted: false,
      rejected: false,
      isEmergency: true,
    };
    this.proposals.set(proposal.id, proposal);
    return proposal;
  }

  getProposal(id: number): ParameterProposal {
    const proposal = this.proposals.get(id);
    if (!proposal) throw new NotFoundError(`Proposal ${id} not found`);
    return proposal;
  }

  listProposals(filter?: { pool?: string; status?: 'pending' | 'accepted' | 'rejected' }) {
    let all = [...this.proposals.values()];
    if (filter?.pool) all = all.filter((p) => p.pool === filter.pool);
    if (filter?.status === 'accepted') all = all.filter((p) => p.accepted);
    if (filter?.status === 'rejected') all = all.filter((p) => p.rejected);
    if (filter?.status === 'pending') all = all.filter((p) => !p.accepted && !p.rejected);
    return all.sort((a, b) => b.id - a.id);
  }

  /**
   * Accepts a proposal once its timelock has elapsed and, when voting is
   * enabled, once it has passed the vote.
   */
  acceptProposal(id: number): ParameterProposal {
    const proposal = this.getProposal(id);
    if (proposal.accepted || proposal.rejected) {
      throw new ValidationError(`Proposal ${id} has already been decided`);
    }
    const now = nowSeconds();
    if (now < proposal.effectiveAt) {
      throw new ValidationError(
        `Timelock has not elapsed: ${proposal.effectiveAt - now}s remaining`,
      );
    }
    if (this.votingConfig) {
      if (this.isVotingOpen(proposal)) {
        throw new ValidationError('Voting is still open for this proposal');
      }
      if (!this.getTally(id).hasPassed) {
        throw new ValidationError('Proposal did not pass the vote');
      }
    }

    proposal.accepted = true;
    this.commitChange(proposal, now, proposal.isEmergency);
    if (proposal.isEmergency) this.emergencyActive = true;
    return proposal;
  }

  rejectProposal(id: number): ParameterProposal {
    const proposal = this.getProposal(id);
    if (proposal.accepted || proposal.rejected) {
      throw new ValidationError(`Proposal ${id} has already been decided`);
    }
    proposal.rejected = true;
    return proposal;
  }

  isEmergencyActive(): boolean {
    return this.emergencyActive;
  }

  clearEmergencyOverride(): void {
    this.emergencyActive = false;
  }

  /**
   * Writes an accepted proposal through: value, new version, audit trail and
   * change notification. Shared by every acceptance path so none of them can
   * record a change differently.
   */
  private commitChange(
    proposal: ParameterProposal,
    timestamp: number,
    isEmergency: boolean,
  ): void {
    const k = key(proposal.pool, proposal.parameter);
    const oldValue = this.values.get(k) ?? 0;
    const version = (this.versions.get(k) ?? 0) + 1;

    this.values.set(k, proposal.proposedValue);
    this.versions.set(k, version);
    this.versionedValues.set(`${k}::${version}`, proposal.proposedValue);

    const change: ParameterChange = {
      parameter: proposal.parameter,
      oldValue,
      newValue: proposal.proposedValue,
      timestamp,
      effectiveAt: proposal.effectiveAt,
      changedBy: proposal.proposer,
      version,
    };
    this.history.set(k, [...(this.history.get(k) ?? []), change]);

    this.notifications.push({
      pool: proposal.pool,
      parameter: proposal.parameter,
      oldValue,
      newValue: proposal.proposedValue,
      version,
      effectiveAt: proposal.effectiveAt,
      isEmergency,
      emittedAt: timestamp,
    });
    logger.info('Parameter changed', {
      pool: proposal.pool,
      parameter: proposal.parameter,
      oldValue,
      newValue: proposal.proposedValue,
      version,
    });
  }

  /** Change notifications, newest first. */
  getNotifications(limit = 50, pool?: string): ParameterChangeNotification[] {
    const filtered = pool
      ? this.notifications.filter((n) => n.pool === pool)
      : this.notifications;
    return [...filtered].reverse().slice(0, limit);
  }

  // ----------------------------------------------------------------- voting

  setVotingConfig(config: VotingConfig): VotingConfig {
    const { quorumBps, approvalThresholdBps, votingPeriodSeconds } = config;
    if (!(quorumBps > 0 && quorumBps <= BPS_DIVISOR)) {
      throw new ValidationError('quorumBps must be between 1 and 10000');
    }
    if (!(approvalThresholdBps > 0 && approvalThresholdBps <= BPS_DIVISOR)) {
      throw new ValidationError('approvalThresholdBps must be between 1 and 10000');
    }
    if (!(votingPeriodSeconds > 0)) {
      throw new ValidationError('votingPeriodSeconds must be positive');
    }
    this.votingConfig = { quorumBps, approvalThresholdBps, votingPeriodSeconds };
    return this.votingConfig;
  }

  getVotingConfig(): VotingConfig | null {
    return this.votingConfig;
  }

  setVotingPower(voter: string, weight: number): void {
    if (!Number.isFinite(weight) || weight < 0) {
      throw new ValidationError('weight must be a non-negative number');
    }
    this.votingPower.set(voter, weight);
  }

  getVotingPower(voter: string): number {
    return this.votingPower.get(voter) ?? 0;
  }

  getTotalVotingPower(): number {
    let total = 0;
    for (const weight of this.votingPower.values()) total += weight;
    return total;
  }

  private isVotingOpen(proposal: ParameterProposal): boolean {
    if (!this.votingConfig) return false;
    return nowSeconds() < proposal.createdAt + this.votingConfig.votingPeriodSeconds;
  }

  castVote(proposalId: number, voter: string, support: boolean): ParameterVote {
    if (!this.votingConfig) throw new ValidationError('Voting is not enabled');
    const proposal = this.getProposal(proposalId);
    if (proposal.accepted || proposal.rejected) {
      throw new ValidationError(`Proposal ${proposalId} has already been decided`);
    }
    if (!this.isVotingOpen(proposal)) {
      throw new ValidationError('Voting is closed for this proposal');
    }

    const weight = this.getVotingPower(voter);
    if (weight <= 0) throw new ValidationError('Voter has no voting power');

    const cast = this.votes.get(proposalId) ?? [];
    if (cast.some((v) => v.voter === voter)) {
      throw new ValidationError('Voter has already voted on this proposal');
    }

    const vote: ParameterVote = {
      proposalId,
      voter,
      support,
      weight,
      votedAt: nowSeconds(),
    };
    this.votes.set(proposalId, [...cast, vote]);
    return vote;
  }

  getVotes(proposalId: number): ParameterVote[] {
    return [...(this.votes.get(proposalId) ?? [])];
  }

  /**
   * Tally for a proposal.
   *
   * Quorum is measured against total registered power; approval is measured
   * against the weight actually cast, so abstaining neither approves nor blocks.
   */
  getTally(proposalId: number): VoteTally {
    const votes = this.getVotes(proposalId);
    const totalVotingPower = this.getTotalVotingPower();
    let forWeight = 0;
    let againstWeight = 0;
    for (const vote of votes) {
      if (vote.support) forWeight += vote.weight;
      else againstWeight += vote.weight;
    }
    const participation = forWeight + againstWeight;
    const config = this.votingConfig;

    // Cross-multiplied rather than dividing first: truncating the required
    // weight would let a 20.00% quorum pass on 19.99% of the power, and a
    // 50/50 split clear a 50.01% approval threshold.
    const meetsQuorum = Boolean(
      config &&
        totalVotingPower > 0 &&
        participation * BPS_DIVISOR >= totalVotingPower * config.quorumBps,
    );
    const isApproved = Boolean(
      config &&
        participation > 0 &&
        forWeight * BPS_DIVISOR >= participation * config.approvalThresholdBps,
    );

    return {
      proposalId,
      forWeight,
      againstWeight,
      voterCount: votes.length,
      totalVotingPower,
      participation,
      meetsQuorum,
      isApproved,
      hasPassed: meetsQuorum && isApproved,
    };
  }

  // ------------------------------------------------------------- simulation

  /**
   * Projects the effect of a proposed value against a snapshot of pool state.
   *
   * A faithful port of `parameter-store/src/simulation.rs` — same formulas,
   * same thresholds, same warning names.
   */
  simulateChange(
    pool: string,
    parameter: ParameterType,
    proposedValue: number,
    snapshot: PoolSnapshot,
  ): ParameterImpact {
    const currentValue = this.getParameter(pool, parameter);
    const ltv = this.getParameter(pool, 'LTV');
    const threshold = this.getParameter(pool, 'LiquidationThreshold');

    const warnings: string[] = [];
    let borrowingPowerDelta = 0;
    let newlyLiquidatableDebt = 0;
    let borrowRateDeltaBps = 0;

    switch (parameter) {
      case 'LTV': {
        borrowingPowerDelta =
          applyBps(snapshot.totalCollateral, proposedValue) -
          applyBps(snapshot.totalCollateral, currentValue);
        if (threshold > 0 && proposedValue >= threshold) {
          warnings.push('ltv_above_threshold');
        }
        break;
      }
      case 'LiquidationThreshold': {
        if (proposedValue > currentValue) {
          const tightening = proposedValue - currentValue;
          if (snapshot.atRiskBandBps > 0) {
            const share = Math.min(tightening, snapshot.atRiskBandBps);
            newlyLiquidatableDebt = applyBps(
              snapshot.atRiskDebt,
              ratioBps(share, snapshot.atRiskBandBps),
            );
          } else {
            newlyLiquidatableDebt = snapshot.atRiskDebt;
          }
          if (newlyLiquidatableDebt > 0) warnings.push('positions_liquidatable');
        }
        if (ltv > 0 && proposedValue <= ltv) warnings.push('threshold_below_ltv');
        break;
      }
      case 'BaseInterestRate': {
        borrowRateDeltaBps = proposedValue - currentValue;
        break;
      }
      case 'Slope1':
      case 'Slope2':
      case 'OptimalUtilization': {
        borrowRateDeltaBps = this.projectRateDelta(
          pool,
          parameter,
          currentValue,
          proposedValue,
          snapshot,
        );
        break;
      }
      case 'DebtCeiling': {
        borrowingPowerDelta = proposedValue - currentValue;
        if (proposedValue < snapshot.totalDebt) warnings.push('ceiling_below_debt');
        break;
      }
      default:
        // CloseFactor, LiquidationIncentive and ReserveFactor change how a
        // liquidation is sized or how revenue splits, not who is liquidatable.
        break;
    }

    const relativeChangeBps =
      currentValue === 0 ? 0 : ratioBps(proposedValue - currentValue, Math.abs(currentValue));

    return {
      parameter,
      currentValue,
      proposedValue,
      relativeChangeBps,
      borrowingPowerDelta,
      newlyLiquidatableDebt,
      borrowRateDeltaBps,
      severity: this.classify(
        parameter,
        relativeChangeBps,
        newlyLiquidatableDebt,
        borrowRateDeltaBps,
        snapshot,
      ),
      warnings,
    };
  }

  private projectRateDelta(
    pool: string,
    parameter: ParameterType,
    currentValue: number,
    proposedValue: number,
    snapshot: PoolSnapshot,
  ): number {
    const utilization =
      snapshot.totalDeposits > 0
        ? Math.min(ratioBps(snapshot.totalDebt, snapshot.totalDeposits), BPS_DIVISOR)
        : 0;

    const base = this.getParameter(pool, 'BaseInterestRate');
    const kink = this.getParameter(pool, 'OptimalUtilization');
    const slope1 = this.getParameter(pool, 'Slope1');
    const slope2 = this.getParameter(pool, 'Slope2');

    const rateWith = (value: number): number => {
      const withKink = parameter === 'OptimalUtilization' ? value : kink;
      const withSlope1 = parameter === 'Slope1' ? value : slope1;
      const withSlope2 = parameter === 'Slope2' ? value : slope2;
      return kinkRate(utilization, base, withKink, withSlope1, withSlope2);
    };

    return rateWith(proposedValue) - rateWith(currentValue);
  }

  /**
   * Grades an impact. Any debt becoming liquidatable is `high` regardless of
   * how small the parameter move looks.
   */
  private classify(
    parameter: ParameterType,
    relativeChangeBps: number,
    newlyLiquidatableDebt: number,
    borrowRateDeltaBps: number,
    snapshot: PoolSnapshot,
  ): ImpactSeverity {
    if (newlyLiquidatableDebt > 0) return 'high';
    if (Math.abs(borrowRateDeltaBps) >= 500) return 'high';
    if (relativeChangeBps === 0 && borrowRateDeltaBps === 0) return 'negligible';

    const magnitude = Math.abs(relativeChangeBps);
    const risky = isRiskParameter(parameter);
    const moderateAt = risky ? 500 : 1_500;
    const highAt = risky ? 2_000 : 5_000;

    if (magnitude >= highAt && snapshot.totalDebt > 0) return 'high';
    if (magnitude >= moderateAt) return 'moderate';
    if (magnitude > 0 || borrowRateDeltaBps !== 0) return 'low';
    return 'negligible';
  }

  /** Test seam: drops all state. */
  reset(): void {
    this.pools.clear();
    this.values.clear();
    this.versions.clear();
    this.versionedValues.clear();
    this.history.clear();
    this.proposals.clear();
    this.votes.clear();
    this.votingPower.clear();
    this.votingConfig = null;
    this.notifications = [];
    this.proposalCounter = 0;
    this.emergencyActive = false;
  }
}

export const parametersService = new ParametersService();
