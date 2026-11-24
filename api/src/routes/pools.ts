/**
 * Pool whitelisting gated by risk-council quorum — Issue #1528.
 *
 * Mounted at `/api/pools`. A pool can only be whitelisted once a quorum of
 * risk-council members has approved it. Any council member may open a proposal,
 * and the proposer's own approval counts toward quorum. Each member approves a
 * proposal at most once, and the proposal whitelists the pool the moment quorum
 * is reached.
 *
 * Council configuration is read from the environment on every request:
 *   RISK_COUNCIL_MEMBERS  comma-separated Stellar addresses
 *   RISK_COUNCIL_QUORUM   optional; defaults to a strict majority of members
 *
 * With no members configured, nobody can propose or approve, so the whitelist
 * fails closed. State is held in memory, like the other governance services in
 * this API; it resets on restart.
 *
 * Route order matters: literal segments (`whitelist/proposals`) are declared
 * before the `whitelist/:pool` lookup.
 */

import { randomUUID } from 'node:crypto';
import { Response, Router } from 'express';
import { authenticateToken, AuthRequest } from '../middleware/auth';
import {
  ConflictError,
  ForbiddenError,
  InternalServerError,
  NotFoundError,
  ValidationError,
} from '../utils/errors';
import logger from '../utils/logger';

export type WhitelistProposalStatus = 'pending' | 'whitelisted';

export interface WhitelistProposal {
  id: string;
  pool: string;
  proposedBy: string;
  /** Unique council members who have approved, in the order they approved. */
  approvals: string[];
  quorum: number;
  status: WhitelistProposalStatus;
  createdAt: string;
  whitelistedAt?: string;
}

export interface RiskCouncil {
  members: string[];
  quorum: number;
}

/** Stellar contract (strkey) address: `C` followed by 55 base32 characters. */
const CONTRACT_ADDRESS_PATTERN = /^C[A-Z2-7]{55}$/;

const proposals = new Map<string, WhitelistProposal>();
const whitelist = new Map<string, { proposalId: string; whitelistedAt: string }>();

export function getRiskCouncil(env: NodeJS.ProcessEnv = process.env): RiskCouncil {
  const members = [
    ...new Set(
      (env.RISK_COUNCIL_MEMBERS ?? '')
        .split(',')
        .map((member) => member.trim())
        .filter((member) => member.length > 0),
    ),
  ];
  const configured = Number(env.RISK_COUNCIL_QUORUM);
  const quorum =
    env.RISK_COUNCIL_QUORUM !== undefined && Number.isInteger(configured) && configured > 0
      ? configured
      : Math.floor(members.length / 2) + 1;
  return { members, quorum };
}

/** Returns the council, or throws if the caller may not act on it. */
function requireCouncilMember(address: string | undefined): RiskCouncil {
  const council = getRiskCouncil();
  if (council.members.length === 0) {
    throw new ForbiddenError('Risk council is not configured');
  }
  if (council.quorum > council.members.length) {
    logger.error('Risk council quorum exceeds membership', council);
    throw new InternalServerError('Risk council is misconfigured');
  }
  if (!address || !council.members.includes(address)) {
    throw new ForbiddenError('Only risk council members can perform this action');
  }
  return council;
}

function normalisePool(value: unknown): string {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new ValidationError('pool is required');
  }
  const pool = value.trim();
  if (!CONTRACT_ADDRESS_PATTERN.test(pool)) {
    throw new ValidationError('pool must be a Stellar contract address');
  }
  return pool;
}

export function isPoolWhitelisted(pool: string): boolean {
  return whitelist.has(pool);
}

export function listWhitelistedPools(): Array<{ pool: string; proposalId: string; whitelistedAt: string }> {
  return [...whitelist.entries()].map(([pool, entry]) => ({ pool, ...entry }));
}

export function listProposals(): WhitelistProposal[] {
  return [...proposals.values()].map((proposal) => ({ ...proposal, approvals: [...proposal.approvals] }));
}

export function getProposal(id: string): WhitelistProposal | undefined {
  const proposal = proposals.get(id);
  return proposal && { ...proposal, approvals: [...proposal.approvals] };
}

export function proposePoolWhitelist(pool: string, proposer: string): WhitelistProposal {
  const council = requireCouncilMember(proposer);
  if (isPoolWhitelisted(pool)) {
    throw new ConflictError('Pool is already whitelisted');
  }
  const duplicate = [...proposals.values()].find(
    (proposal) => proposal.pool === pool && proposal.status === 'pending',
  );
  if (duplicate) {
    throw new ConflictError(`A pending proposal already exists for this pool: ${duplicate.id}`);
  }

  const proposal: WhitelistProposal = {
    id: randomUUID(),
    pool,
    proposedBy: proposer,
    approvals: [proposer],
    quorum: council.quorum,
    status: 'pending',
    createdAt: new Date().toISOString(),
  };
  proposals.set(proposal.id, proposal);
  logger.info('Pool whitelist proposed', { proposalId: proposal.id, pool, proposer });
  return finaliseIfQuorumReached(proposal);
}

export function approvePoolWhitelist(proposalId: string, approver: string): WhitelistProposal {
  requireCouncilMember(approver);
  const proposal = proposals.get(proposalId);
  if (!proposal) {
    throw new NotFoundError('Whitelist proposal not found');
  }
  if (proposal.status === 'whitelisted') {
    throw new ConflictError('Proposal has already been executed');
  }
  if (proposal.approvals.includes(approver)) {
    throw new ConflictError('Council member has already approved this proposal');
  }

  proposal.approvals.push(approver);
  logger.info('Pool whitelist approved', {
    proposalId,
    pool: proposal.pool,
    approver,
    approvals: proposal.approvals.length,
    quorum: proposal.quorum,
  });
  return finaliseIfQuorumReached(proposal);
}

function finaliseIfQuorumReached(proposal: WhitelistProposal): WhitelistProposal {
  if (proposal.approvals.length >= proposal.quorum) {
    proposal.status = 'whitelisted';
    proposal.whitelistedAt = new Date().toISOString();
    whitelist.set(proposal.pool, {
      proposalId: proposal.id,
      whitelistedAt: proposal.whitelistedAt,
    });
    logger.info('Pool whitelisted by risk council quorum', {
      proposalId: proposal.id,
      pool: proposal.pool,
      approvals: proposal.approvals.length,
    });
  }
  return { ...proposal, approvals: [...proposal.approvals] };
}

/** Test helper: clears proposals and the whitelist. */
export function resetPoolWhitelistStore(): void {
  proposals.clear();
  whitelist.clear();
}

const router: Router = Router();

/**
 * @openapi
 * /pools/whitelist:
 *   get:
 *     summary: List pools whitelisted by risk-council quorum
 *     tags:
 *       - Pools
 *     responses:
 *       200:
 *         description: Whitelisted pools
 */
router.get('/whitelist', (_req, res: Response) => {
  res.json({ success: true, data: listWhitelistedPools() });
});

/**
 * @openapi
 * /pools/whitelist/proposals:
 *   get:
 *     summary: List whitelist proposals
 *     tags:
 *       - Pools
 *     responses:
 *       200:
 *         description: Proposals, pending and executed
 */
router.get('/whitelist/proposals', (_req, res: Response) => {
  res.json({ success: true, data: listProposals() });
});

/**
 * @openapi
 * /pools/whitelist/proposals/{id}:
 *   get:
 *     summary: Get one whitelist proposal
 *     tags:
 *       - Pools
 *     parameters:
 *       - in: path
 *         name: id
 *         required: true
 *         schema:
 *           type: string
 *     responses:
 *       200:
 *         description: The proposal
 *       404:
 *         description: Proposal not found
 */
router.get('/whitelist/proposals/:id', (req, res: Response) => {
  const proposal = getProposal(req.params.id);
  if (!proposal) {
    throw new NotFoundError('Whitelist proposal not found');
  }
  res.json({ success: true, data: proposal });
});

/**
 * @openapi
 * /pools/whitelist/proposals:
 *   post:
 *     summary: Propose whitelisting a pool (risk council members only)
 *     tags:
 *       - Pools
 *     security:
 *       - bearerAuth: []
 *     requestBody:
 *       required: true
 *       content:
 *         application/json:
 *           schema:
 *             type: object
 *             required: [pool]
 *             properties:
 *               pool:
 *                 type: string
 *     responses:
 *       201:
 *         description: Proposal created; the proposer's approval counts toward quorum
 *       400:
 *         description: Invalid pool address
 *       403:
 *         description: Caller is not a risk council member
 *       409:
 *         description: Pool already whitelisted or a proposal is already pending
 */
router.post('/whitelist/proposals', authenticateToken, (req: AuthRequest, res: Response) => {
  const pool = normalisePool(req.body?.pool);
  const proposal = proposePoolWhitelist(pool, req.user?.address ?? '');
  res.status(201).json({ success: true, data: proposal });
});

/**
 * @openapi
 * /pools/whitelist/proposals/{id}/approvals:
 *   post:
 *     summary: Approve a whitelist proposal (risk council members only)
 *     tags:
 *       - Pools
 *     security:
 *       - bearerAuth: []
 *     parameters:
 *       - in: path
 *         name: id
 *         required: true
 *         schema:
 *           type: string
 *     responses:
 *       200:
 *         description: Approval recorded; status becomes whitelisted once quorum is reached
 *       403:
 *         description: Caller is not a risk council member
 *       404:
 *         description: Proposal not found
 *       409:
 *         description: Already approved by this member, or already executed
 */
router.post(
  '/whitelist/proposals/:id/approvals',
  authenticateToken,
  (req: AuthRequest, res: Response) => {
    const proposal = approvePoolWhitelist(req.params.id, req.user?.address ?? '');
    res.json({ success: true, data: proposal });
  },
);

/**
 * @openapi
 * /pools/whitelist/{pool}:
 *   get:
 *     summary: Whitelist status for one pool
 *     tags:
 *       - Pools
 *     parameters:
 *       - in: path
 *         name: pool
 *         required: true
 *         schema:
 *           type: string
 *     responses:
 *       200:
 *         description: Whitelist status
 */
router.get('/whitelist/:pool', (req, res: Response) => {
  const pool = normalisePool(req.params.pool);
  const entry = whitelist.get(pool);
  res.json({
    success: true,
    data: { pool, whitelisted: entry !== undefined, ...(entry ?? {}) },
  });
});

export default router;
