/**
 * Shared E2E test harness — Issue #485
 *
 * Extends the mocked oracle/contract/API pattern established in
 * `pipeline.e2e.test.ts` with a minimal, in-memory lending state machine so
 * the full user journey (deposit → borrow → check position → repay →
 * withdraw), multi-collateral positions, oracle-driven borrow/liquidation
 * limits, and role-gated admin/liquidator actions can be exercised without a
 * live Soroban devnet, API, or oracle process. Every store resets via
 * `reset()` between tests.
 */

import express, { Application, NextFunction, Request, Response } from 'express';
import request from 'supertest';

// ─── Roles ──────────────────────────────────────────────────────────────────

export type Role = 'admin' | 'user' | 'liquidator';

const roles = new Map<string, Role>();

export function assignRole(address: string, role: Role): void {
  roles.set(address, role);
}

function requireRole(role: Role) {
  return (req: Request, res: Response, next: NextFunction) => {
    const caller = (req.body?.callerAddress || req.query.callerAddress) as string | undefined;
    if (!caller || roles.get(caller) !== role) {
      res.status(403).json({ error: `caller must have the '${role}' role` });
      return;
    }
    next();
  };
}

// ─── Oracle price store ─────────────────────────────────────────────────────

const prices = new Map<string, number>();

export function setPrice(asset: string, price: number): void {
  prices.set(asset.toUpperCase(), price);
}

function getPrice(asset: string): number {
  return prices.get(asset.toUpperCase()) ?? 0;
}

// ─── Protocol pause state ───────────────────────────────────────────────────

let paused = false;

// ─── Positions ──────────────────────────────────────────────────────────────

export interface Position {
  userAddress: string;
  collateral: Record<string, number>;
  debt: Record<string, number>;
}

const positions = new Map<string, Position>();

function getOrCreatePosition(userAddress: string): Position {
  let position = positions.get(userAddress);
  if (!position) {
    position = { userAddress, collateral: {}, debt: {} };
    positions.set(userAddress, position);
  }
  return position;
}

// Same collateral factor and liquidation threshold applied to every asset —
// enough to model cross-asset borrow capacity and liquidation eligibility
// without needing a full per-asset risk-parameter table for this harness.
const MAX_LTV = 0.75;
const LIQUIDATION_THRESHOLD = 0.85;
const CLOSE_FACTOR = 0.5; // max fraction of debt a liquidator may repay per call
const LIQUIDATION_BONUS = 0.1;

function collateralValue(position: Position): number {
  return Object.entries(position.collateral).reduce(
    (sum, [asset, amount]) => sum + amount * getPrice(asset),
    0
  );
}

function debtValue(position: Position): number {
  return Object.entries(position.debt).reduce((sum, [asset, amount]) => sum + amount * getPrice(asset), 0);
}

function healthFactor(position: Position): number {
  const debt = debtValue(position);
  if (debt === 0) return Number.POSITIVE_INFINITY;
  return (collateralValue(position) * LIQUIDATION_THRESHOLD) / debt;
}

export function positionSummary(userAddress: string) {
  const position = getOrCreatePosition(userAddress);
  return {
    userAddress,
    collateral: { ...position.collateral },
    debt: { ...position.debt },
    collateralValueUsd: collateralValue(position),
    debtValueUsd: debtValue(position),
    healthFactor: healthFactor(position),
    liquidatable: healthFactor(position) < 1,
  };
}

export interface CrossContractScenarioStep {
  name: string;
  method: 'get' | 'post';
  path: string;
  body?: Record<string, unknown>;
  expectStatus?: number;
  assert?: (body: unknown) => void;
}

export async function runCrossContractScenario(
  app: Application,
  steps: CrossContractScenarioStep[]
): Promise<unknown[]> {
  const responses: unknown[] = [];

  for (const step of steps) {
    const response =
      step.method === 'get'
        ? await request(app).get(step.path)
        : await request(app).post(step.path).send(step.body ?? {});

    expect(response.status).toBe(step.expectStatus ?? 200);
    step.assert?.(response.body);
    responses.push(response.body);
  }

  return responses;
}

// ─── Governance ─────────────────────────────────────────────────────────────
// Mirrors the on-chain lifecycle in `hello-world/src/governance/`:
// Pending → Active → Queued → Executed | Defeated | Cancelled | Expired.

export enum GovProposalStatus {
  Pending = 'Pending',
  Active = 'Active',
  Succeeded = 'Succeeded',
  Defeated = 'Defeated',
  Expired = 'Expired',
  Queued = 'Queued',
  Executed = 'Executed',
  Cancelled = 'Cancelled',
}

const BASIS_POINTS_SCALE = 10_000;

export interface GovConfig {
  votingPeriod: number;
  executionDelay: number;
  quorumBps: number;
  proposalThreshold: number;
  timelockDuration: number;
  defaultVotingThreshold: number;
}

export interface GovProposal {
  id: number;
  proposer: string;
  proposalType: string;
  description: string;
  status: GovProposalStatus;
  startTime: number;
  endTime: number;
  executionTime: number | null;
  votingThreshold: number;
  forVotes: number;
  againstVotes: number;
  abstainVotes: number;
  totalVotingPower: number;
  createdAt: number;
  executedAt: number | null;
  events: string[];
}

let govInitialized = false;
let govConfig: GovConfig | null = null;
let govNow = 0;
let nextProposalId = 0;
const govProposals = new Map<number, GovProposal>();
const govVotes = new Map<string, string>(); // `${proposalId}:${voter}` → voteType
const govBalances = new Map<string, number>(); // address → vote-token balance

export function initializeGovernance(config: Partial<GovConfig> & { balances?: Record<string, number> }): void {
  govInitialized = true;
  govConfig = {
    votingPeriod: config.votingPeriod ?? 100,
    executionDelay: config.executionDelay ?? 50,
    quorumBps: config.quorumBps ?? 4000,
    proposalThreshold: config.proposalThreshold ?? 100,
    timelockDuration: config.timelockDuration ?? 200,
    defaultVotingThreshold: config.defaultVotingThreshold ?? 5000,
  };
  govNow = 0;
  nextProposalId = 0;
  govProposals.clear();
  govVotes.clear();
  govBalances.clear();
  for (const [addr, bal] of Object.entries(config.balances ?? {})) {
    govBalances.set(addr, bal);
  }
}

function govBalance(address: string): number {
  return govBalances.get(address) ?? 0;
}

function getGovProposal(id: number): GovProposal | undefined {
  return govProposals.get(id);
}

export function buildGovernanceApp(): Application {
  const app = express();
  app.use(express.json());

  app.post('/api/governance/initialize', (req, res) => {
    if (govInitialized) {
      res.status(409).json({ error: 'governance already initialized' });
      return;
    }
    initializeGovernance(req.body ?? {});
    res.json({ success: true, config: govConfig });
  });

  app.get('/api/governance/config', (_req, res) => {
    if (!govConfig) {
      res.status(404).json({ error: 'governance not initialized' });
      return;
    }
    res.json(govConfig);
  });

  app.post('/api/governance/advance-time', (req, res) => {
    const { seconds } = req.body;
    if (!(seconds > 0)) {
      res.status(400).json({ error: 'positive seconds required' });
      return;
    }
    govNow += seconds;
    res.json({ success: true, now: govNow });
  });

  app.post('/api/governance/proposals', (req, res) => {
    if (!govConfig) {
      res.status(404).json({ error: 'governance not initialized' });
      return;
    }
    const { proposer, proposalType, description, votingThreshold } = req.body;
    if (!proposer || !proposalType || !description) {
      res.status(400).json({ error: 'proposer, proposalType, and description are required' });
      return;
    }
    if (govConfig.proposalThreshold > 0 && govBalance(proposer) < govConfig.proposalThreshold) {
      res.status(400).json({ error: 'insufficient proposal power for threshold' });
      return;
    }
    const id = nextProposalId++;
    const proposal: GovProposal = {
      id,
      proposer,
      proposalType,
      description,
      status: GovProposalStatus.Pending,
      startTime: govNow,
      endTime: govNow + govConfig.votingPeriod,
      executionTime: null,
      votingThreshold: votingThreshold ?? govConfig.defaultVotingThreshold,
      forVotes: 0,
      againstVotes: 0,
      abstainVotes: 0,
      totalVotingPower: 0,
      createdAt: govNow,
      executedAt: null,
      events: ['created'],
    };
    govProposals.set(id, proposal);
    res.json({ success: true, proposalId: id, proposal });
  });

  app.get('/api/governance/proposals/:id', (req, res) => {
    const proposal = getGovProposal(Number(req.params.id));
    if (!proposal) {
      res.status(404).json({ error: 'proposal not found' });
      return;
    }
    res.json(proposal);
  });

  app.post('/api/governance/proposals/:id/vote', (req, res) => {
    const proposal = getGovProposal(Number(req.params.id));
    if (!proposal) {
      res.status(404).json({ error: 'proposal not found' });
      return;
    }
    const { voter, voteType } = req.body;
    if (!voter || !['For', 'Against', 'Abstain'].includes(voteType)) {
      res.status(400).json({ error: 'voter and voteType (For|Against|Abstain) are required' });
      return;
    }
    if (proposal.status === GovProposalStatus.Pending && govNow >= proposal.startTime) {
      proposal.status = GovProposalStatus.Active;
    }
    if (proposal.status !== GovProposalStatus.Active) {
      res.status(409).json({ error: 'proposal is not active' });
      return;
    }
    const voteKey = `${proposal.id}:${voter}`;
    if (govVotes.has(voteKey)) {
      res.status(409).json({ error: 'voter has already voted' });
      return;
    }
    const power = govBalance(voter);
    if (power === 0) {
      res.status(400).json({ error: 'voter has no voting power' });
      return;
    }
    if (voteType === 'For') proposal.forVotes += power;
    else if (voteType === 'Against') proposal.againstVotes += power;
    else proposal.abstainVotes += power;
    proposal.totalVotingPower += power;
    govVotes.set(voteKey, voteType);
    proposal.events.push('voted');
    res.json({ success: true, proposal });
  });

  app.post('/api/governance/proposals/:id/queue', (req, res) => {
    const proposal = getGovProposal(Number(req.params.id));
    if (!proposal) {
      res.status(404).json({ error: 'proposal not found' });
      return;
    }
    if (!govConfig) {
      res.status(404).json({ error: 'governance not initialized' });
      return;
    }
    if (govNow <= proposal.endTime) {
      res.status(409).json({ error: 'voting has not ended' });
      return;
    }
    if (
      proposal.status === GovProposalStatus.Executed ||
      proposal.status === GovProposalStatus.Cancelled ||
      proposal.status === GovProposalStatus.Expired ||
      proposal.status === GovProposalStatus.Queued
    ) {
      res.status(409).json({ error: 'invalid proposal status for queueing' });
      return;
    }

    const totalVotes =
      proposal.forVotes + proposal.againstVotes + proposal.abstainVotes;
    const quorumRequired = Math.floor((totalVotes * govConfig.quorumBps) / BASIS_POINTS_SCALE);
    const quorumReached = totalVotes >= quorumRequired;
    const thresholdVotes = Math.floor(
      (proposal.totalVotingPower * proposal.votingThreshold) / BASIS_POINTS_SCALE
    );
    const thresholdMet = proposal.forVotes >= thresholdVotes;
    const succeeded = quorumReached && thresholdMet;

    if (succeeded) {
      proposal.executionTime = govNow + govConfig.executionDelay;
      proposal.status = GovProposalStatus.Queued;
      proposal.events.push('queued');
    } else {
      proposal.status = GovProposalStatus.Defeated;
      proposal.events.push('defeated');
    }

    res.json({
      success: true,
      succeeded,
      quorumReached,
      quorumRequired,
      thresholdMet,
      proposal,
    });
  });

  app.post('/api/governance/proposals/:id/execute', (req, res) => {
    const proposal = getGovProposal(Number(req.params.id));
    if (!proposal) {
      res.status(404).json({ error: 'proposal not found' });
      return;
    }
    if (!govConfig) {
      res.status(404).json({ error: 'governance not initialized' });
      return;
    }
    if (proposal.status !== GovProposalStatus.Queued) {
      res.status(409).json({ error: 'proposal is not queued' });
      return;
    }
    if (proposal.executionTime === null) {
      res.status(409).json({ error: 'missing execution time' });
      return;
    }
    if (govNow < proposal.executionTime) {
      res.status(409).json({ error: 'execution too early: delay not elapsed' });
      return;
    }
    if (govNow > proposal.executionTime + govConfig.timelockDuration) {
      proposal.status = GovProposalStatus.Expired;
      proposal.events.push('expired');
      res.status(410).json({ error: 'proposal expired: timelock window passed', proposal });
      return;
    }
    proposal.status = GovProposalStatus.Executed;
    proposal.executedAt = govNow;
    proposal.events.push('executed');
    res.json({ success: true, proposal });
  });

  app.post('/api/governance/proposals/:id/cancel', (req, res) => {
    const proposal = getGovProposal(Number(req.params.id));
    if (!proposal) {
      res.status(404).json({ error: 'proposal not found' });
      return;
    }
    const { caller } = req.body;
    if (!caller) {
      res.status(400).json({ error: 'caller is required' });
      return;
    }
    if (caller !== proposal.proposer && caller !== 'GADMIN') {
      res.status(403).json({ error: 'unauthorized: only proposer or admin may cancel' });
      return;
    }
    if (
      proposal.status === GovProposalStatus.Executed ||
      proposal.status === GovProposalStatus.Queued
    ) {
      res.status(409).json({ error: 'cannot cancel executed or queued proposal' });
      return;
    }
    proposal.status = GovProposalStatus.Cancelled;
    proposal.events.push('cancelled');
    res.json({ success: true, proposal });
  });

  return app;
}

// ─── App ────────────────────────────────────────────────────────────────────

export function buildLendingApp(): Application {
  const app = express();
  app.use(express.json());

  app.get('/api/health', (_req, res) => {
    res.json({ status: paused ? 'paused' : 'healthy' });
  });

  app.get('/api/prices/:asset', (req, res) => {
    const asset = req.params.asset.toUpperCase();
    if (!prices.has(asset)) {
      res.status(404).json({ error: `no price for ${asset}` });
      return;
    }
    res.json({ asset, price: prices.get(asset) });
  });

  function guardPaused(res: Response): boolean {
    if (paused) {
      res.status(503).json({ error: 'protocol is paused' });
      return true;
    }
    return false;
  }

  app.post('/api/lending/deposit', (req, res) => {
    if (guardPaused(res)) return;
    const { userAddress, asset, amount } = req.body;
    if (!userAddress || !asset || !(amount > 0)) {
      res.status(400).json({ error: 'userAddress, asset, and a positive amount are required' });
      return;
    }
    const position = getOrCreatePosition(userAddress);
    position.collateral[asset] = (position.collateral[asset] ?? 0) + amount;
    res.json({ success: true, position: positionSummary(userAddress) });
  });

  app.post('/api/lending/borrow', (req, res) => {
    if (guardPaused(res)) return;
    const { userAddress, asset, amount } = req.body;
    if (!userAddress || !asset || !(amount > 0)) {
      res.status(400).json({ error: 'userAddress, asset, and a positive amount are required' });
      return;
    }
    if (getPrice(asset) === 0) {
      res.status(400).json({ error: `no oracle price available for ${asset}` });
      return;
    }
    const position = getOrCreatePosition(userAddress);
    const requestedValue = amount * getPrice(asset);
    const maxBorrowValue = collateralValue(position) * MAX_LTV;
    const projectedDebtValue = debtValue(position) + requestedValue;
    if (projectedDebtValue > maxBorrowValue) {
      res.status(400).json({
        error: 'borrow exceeds available collateral capacity',
        maxBorrowValue,
        projectedDebtValue,
      });
      return;
    }
    position.debt[asset] = (position.debt[asset] ?? 0) + amount;
    res.json({ success: true, position: positionSummary(userAddress) });
  });

  app.get('/api/positions/:userAddress', (req, res) => {
    res.json(positionSummary(req.params.userAddress));
  });

  app.post('/api/lending/repay', (req, res) => {
    if (guardPaused(res)) return;
    const { userAddress, asset, amount } = req.body;
    const position = getOrCreatePosition(userAddress);
    const owed = position.debt[asset] ?? 0;
    if (!(amount > 0) || amount > owed) {
      res.status(400).json({ error: `repay amount must be > 0 and <= outstanding debt (${owed})` });
      return;
    }
    position.debt[asset] = owed - amount;
    if (position.debt[asset] === 0) delete position.debt[asset];
    res.json({ success: true, position: positionSummary(userAddress) });
  });

  app.post('/api/lending/withdraw', (req, res) => {
    if (guardPaused(res)) return;
    const { userAddress, asset, amount } = req.body;
    const position = getOrCreatePosition(userAddress);
    const held = position.collateral[asset] ?? 0;
    if (!(amount > 0) || amount > held) {
      res.status(400).json({ error: `withdraw amount must be > 0 and <= held collateral (${held})` });
      return;
    }

    // Simulate the withdrawal, then verify it doesn't break solvency.
    const projected: Position = {
      ...position,
      collateral: { ...position.collateral, [asset]: held - amount },
    };
    if (debtValue(projected) > 0 && healthFactor(projected) < 1) {
      res.status(400).json({ error: 'withdrawal would leave the position undercollateralized' });
      return;
    }

    position.collateral[asset] = held - amount;
    if (position.collateral[asset] === 0) delete position.collateral[asset];
    res.json({ success: true, position: positionSummary(userAddress) });
  });

  app.post('/api/liquidations/liquidate', requireRole('liquidator'), (req, res) => {
    if (guardPaused(res)) return;
    const { targetUser, debtAsset, collateralAsset, repayAmount } = req.body;
    const position = getOrCreatePosition(targetUser);

    if (!position.debt[debtAsset]) {
      res.status(400).json({ error: `target has no ${debtAsset} debt` });
      return;
    }
    if (healthFactor(position) >= 1) {
      res.status(400).json({ error: 'target position is not liquidatable' });
      return;
    }

    const maxRepay = position.debt[debtAsset]! * CLOSE_FACTOR;
    if (!(repayAmount > 0) || repayAmount > maxRepay) {
      res.status(400).json({ error: `repayAmount must be > 0 and <= close-factor-limited max (${maxRepay})` });
      return;
    }

    const seizedValue = repayAmount * getPrice(debtAsset) * (1 + LIQUIDATION_BONUS);
    const seizedAmount = seizedValue / getPrice(collateralAsset);
    const heldCollateral = position.collateral[collateralAsset] ?? 0;
    if (seizedAmount > heldCollateral) {
      res.status(400).json({ error: 'insufficient collateral of the requested type to seize' });
      return;
    }

    position.debt[debtAsset] = position.debt[debtAsset]! - repayAmount;
    if (position.debt[debtAsset] === 0) delete position.debt[debtAsset];
    position.collateral[collateralAsset] = heldCollateral - seizedAmount;
    if (position.collateral[collateralAsset] === 0) delete position.collateral[collateralAsset];

    res.json({
      success: true,
      repaidAmount: repayAmount,
      seizedAmount,
      position: positionSummary(targetUser),
    });
  });

  app.post('/api/protocol/pause', requireRole('admin'), (_req, res) => {
    paused = true;
    res.json({ success: true, paused });
  });

  app.post('/api/protocol/resume', requireRole('admin'), (_req, res) => {
    paused = false;
    res.json({ success: true, paused });
  });

  app.post('/api/oracle/update-price', (req, res) => {
    const { asset, price } = req.body;
    if (!asset || !(price > 0)) {
      res.status(400).json({ error: 'asset and a positive price are required' });
      return;
    }
    setPrice(asset, price);
    res.json({ success: true, asset: asset.toUpperCase(), price });
  });

  return app;
}

/** Resets every in-memory store — call from `beforeEach` in each scenario file. */
export function reset(): void {
  roles.clear();
  prices.clear();
  positions.clear();
  paused = false;
  govInitialized = false;
  govConfig = null;
  govNow = 0;
  nextProposalId = 0;
  govProposals.clear();
  govVotes.clear();
  govBalances.clear();
}
