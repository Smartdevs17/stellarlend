/**
 * Governance Parameter Store Controller — Issue #697
 *
 * Validates and shapes requests for `services/parameters/parameters.service`.
 * All governance semantics (timelocks, voting rules, versioning, simulation)
 * belong to the service and, ultimately, to the `parameter-store` contract.
 */

import { Request, Response } from 'express';
import {
  parametersService,
  PARAMETER_TYPES,
  ParameterType,
  PoolSnapshot,
} from '../services/parameters/parameters.service';
import { ValidationError, NotFoundError } from '../utils/errors';
import logger from '../utils/logger';

function requireString(value: unknown, name: string): string {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new ValidationError(`${name} is required`);
  }
  return value.trim();
}

function requireNumber(value: unknown, name: string): number {
  const parsed = Number(value);
  if (value === undefined || value === null || value === '' || !Number.isFinite(parsed)) {
    throw new ValidationError(`${name} must be a number`);
  }
  return parsed;
}

function requireParameterType(value: unknown): ParameterType {
  const name = requireString(value, 'parameter');
  if (!PARAMETER_TYPES.includes(name as ParameterType)) {
    throw new ValidationError(
      `Unknown parameter "${name}". Expected one of: ${PARAMETER_TYPES.join(', ')}`,
    );
  }
  return name as ParameterType;
}

/**
 * Reads a pool snapshot from a request body.
 *
 * Every field defaults to 0 so a caller can simulate against a partially known
 * pool: an unknown field contributes nothing to the projection rather than
 * rejecting the request.
 */
function readSnapshot(body: Record<string, unknown>): PoolSnapshot {
  const snapshot = (body.snapshot ?? {}) as Record<string, unknown>;
  const read = (name: string): number => {
    const raw = snapshot[name];
    if (raw === undefined || raw === null || raw === '') return 0;
    const value = Number(raw);
    if (!Number.isFinite(value) || value < 0) {
      throw new ValidationError(`snapshot.${name} must be a non-negative number`);
    }
    return value;
  };
  return {
    totalCollateral: read('totalCollateral'),
    totalDebt: read('totalDebt'),
    totalDeposits: read('totalDeposits'),
    atRiskDebt: read('atRiskDebt'),
    atRiskBandBps: read('atRiskBandBps'),
  };
}

export class ParametersController {
  /** POST /api/parameters/pools — register a pool with the store. */
  async registerPool(req: Request, res: Response): Promise<void> {
    try {
      const pool = requireString(req.body?.pool, 'pool');
      parametersService.registerPool(pool);
      res.status(201).json({ success: true, data: { pool } });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to register pool');
    }
  }

  /** GET /api/parameters/pools */
  async listPools(_req: Request, res: Response): Promise<void> {
    try {
      res.json({ success: true, data: parametersService.listPools() });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to list pools');
    }
  }

  /** GET /api/parameters/:pool — every parameter currently set for a pool. */
  async getPoolParameters(req: Request, res: Response): Promise<void> {
    try {
      const pool = requireString(req.params.pool, 'pool');
      const values = parametersService.getPoolParameters(pool);
      const versions = Object.fromEntries(
        PARAMETER_TYPES.map((p) => [p, parametersService.getVersion(pool, p)]),
      );
      res.json({ success: true, data: { pool, values, versions } });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to fetch pool parameters');
    }
  }

  /** GET /api/parameters/:pool/:parameter */
  async getParameter(req: Request, res: Response): Promise<void> {
    try {
      const pool = requireString(req.params.pool, 'pool');
      const parameter = requireParameterType(req.params.parameter);
      res.json({
        success: true,
        data: {
          pool,
          parameter,
          value: parametersService.getParameter(pool, parameter),
          version: parametersService.getVersion(pool, parameter),
        },
      });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to fetch parameter');
    }
  }

  /** GET /api/parameters/:pool/:parameter/history */
  async getHistory(req: Request, res: Response): Promise<void> {
    try {
      const pool = requireString(req.params.pool, 'pool');
      const parameter = requireParameterType(req.params.parameter);
      const history = parametersService.getHistory(pool, parameter);
      res.json({ success: true, data: history, meta: { count: history.length } });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to fetch parameter history');
    }
  }

  /** GET /api/parameters/:pool/:parameter/versions/:version */
  async getVersion(req: Request, res: Response): Promise<void> {
    try {
      const pool = requireString(req.params.pool, 'pool');
      const parameter = requireParameterType(req.params.parameter);
      const version = requireNumber(req.params.version, 'version');
      if (!Number.isInteger(version) || version <= 0) {
        throw new ValidationError('version must be a positive integer');
      }
      res.json({
        success: true,
        data: {
          pool,
          parameter,
          version,
          value: parametersService.getParameterAtVersion(pool, parameter, version),
        },
      });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to fetch parameter version');
    }
  }

  /** POST /api/parameters/validate */
  async validate(req: Request, res: Response): Promise<void> {
    try {
      const pool = requireString(req.body?.pool, 'pool');
      const parameter = requireParameterType(req.body?.parameter);
      const value = requireNumber(req.body?.value, 'value');
      res.json({
        success: true,
        data: parametersService.validateValue(pool, parameter, value),
      });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to validate parameter value');
    }
  }

  /** POST /api/parameters/simulate — project a change before proposing it. */
  async simulate(req: Request, res: Response): Promise<void> {
    try {
      const body = (req.body ?? {}) as Record<string, unknown>;
      const pool = requireString(body.pool, 'pool');
      const parameter = requireParameterType(body.parameter);
      const value = requireNumber(body.value, 'value');
      const impact = parametersService.simulateChange(
        pool,
        parameter,
        value,
        readSnapshot(body),
      );
      res.json({ success: true, data: impact });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to simulate parameter change');
    }
  }

  /** POST /api/parameters/proposals */
  async propose(req: Request, res: Response): Promise<void> {
    try {
      const body = (req.body ?? {}) as Record<string, unknown>;
      const proposal = parametersService.proposeChange({
        pool: requireString(body.pool, 'pool'),
        parameter: requireParameterType(body.parameter),
        value: requireNumber(body.value, 'value'),
        proposer: requireString(body.proposer, 'proposer'),
        timelockSeconds:
          body.timelockSeconds === undefined
            ? undefined
            : requireNumber(body.timelockSeconds, 'timelockSeconds'),
      });
      res.status(201).json({ success: true, data: proposal });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to create parameter proposal');
    }
  }

  /** POST /api/parameters/proposals/emergency */
  async proposeEmergency(req: Request, res: Response): Promise<void> {
    try {
      const body = (req.body ?? {}) as Record<string, unknown>;
      const proposal = parametersService.proposeEmergencyChange({
        pool: requireString(body.pool, 'pool'),
        parameter: requireParameterType(body.parameter),
        value: requireNumber(body.value, 'value'),
        proposer: requireString(body.proposer, 'proposer'),
      });
      res.status(201).json({ success: true, data: proposal });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to create emergency proposal');
    }
  }

  /** GET /api/parameters/proposals?pool=&status= */
  async listProposals(req: Request, res: Response): Promise<void> {
    try {
      const status = req.query.status as string | undefined;
      if (status && !['pending', 'accepted', 'rejected'].includes(status)) {
        throw new ValidationError('status must be pending, accepted or rejected');
      }
      const proposals = parametersService.listProposals({
        pool: (req.query.pool as string) || undefined,
        status: status as 'pending' | 'accepted' | 'rejected' | undefined,
      });
      res.json({ success: true, data: proposals, meta: { count: proposals.length } });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to list proposals');
    }
  }

  /** GET /api/parameters/proposals/:id */
  async getProposal(req: Request, res: Response): Promise<void> {
    try {
      const id = requireNumber(req.params.id, 'id');
      res.json({ success: true, data: parametersService.getProposal(id) });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to fetch proposal');
    }
  }

  /** POST /api/parameters/proposals/:id/accept */
  async accept(req: Request, res: Response): Promise<void> {
    try {
      const id = requireNumber(req.params.id, 'id');
      res.json({ success: true, data: parametersService.acceptProposal(id) });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to accept proposal');
    }
  }

  /** POST /api/parameters/proposals/:id/reject */
  async reject(req: Request, res: Response): Promise<void> {
    try {
      const id = requireNumber(req.params.id, 'id');
      res.json({ success: true, data: parametersService.rejectProposal(id) });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to reject proposal');
    }
  }

  /** POST /api/parameters/proposals/:id/votes */
  async castVote(req: Request, res: Response): Promise<void> {
    try {
      const id = requireNumber(req.params.id, 'id');
      const voter = requireString(req.body?.voter, 'voter');
      if (typeof req.body?.support !== 'boolean') {
        throw new ValidationError('support must be a boolean');
      }
      const vote = parametersService.castVote(id, voter, req.body.support);
      res.status(201).json({ success: true, data: vote });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to cast vote');
    }
  }

  /** GET /api/parameters/proposals/:id/votes */
  async getVotes(req: Request, res: Response): Promise<void> {
    try {
      const id = requireNumber(req.params.id, 'id');
      const votes = parametersService.getVotes(id);
      res.json({
        success: true,
        data: { votes, tally: parametersService.getTally(id) },
      });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to fetch votes');
    }
  }

  /** GET /api/parameters/voting/config */
  async getVotingConfig(_req: Request, res: Response): Promise<void> {
    try {
      const config = parametersService.getVotingConfig();
      res.json({
        success: true,
        data: { enabled: config !== null, config },
      });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to fetch voting config');
    }
  }

  /** PUT /api/parameters/voting/config — governance-controlled. */
  async setVotingConfig(req: Request, res: Response): Promise<void> {
    try {
      const body = (req.body ?? {}) as Record<string, unknown>;
      const config = parametersService.setVotingConfig({
        quorumBps: requireNumber(body.quorumBps, 'quorumBps'),
        approvalThresholdBps: requireNumber(body.approvalThresholdBps, 'approvalThresholdBps'),
        votingPeriodSeconds: requireNumber(body.votingPeriodSeconds, 'votingPeriodSeconds'),
      });
      res.json({ success: true, data: config });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to set voting config');
    }
  }

  /** PUT /api/parameters/voting/power — governance-controlled. */
  async setVotingPower(req: Request, res: Response): Promise<void> {
    try {
      const voter = requireString(req.body?.voter, 'voter');
      const weight = requireNumber(req.body?.weight, 'weight');
      parametersService.setVotingPower(voter, weight);
      res.json({
        success: true,
        data: { voter, weight, totalVotingPower: parametersService.getTotalVotingPower() },
      });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to set voting power');
    }
  }

  /** GET /api/parameters/notifications?pool=&limit= */
  async getNotifications(req: Request, res: Response): Promise<void> {
    try {
      const limit = req.query.limit === undefined ? 50 : requireNumber(req.query.limit, 'limit');
      if (!Number.isInteger(limit) || limit <= 0 || limit > 500) {
        throw new ValidationError('limit must be an integer between 1 and 500');
      }
      const notifications = parametersService.getNotifications(
        limit,
        (req.query.pool as string) || undefined,
      );
      res.json({
        success: true,
        data: notifications,
        meta: { count: notifications.length },
      });
    } catch (error) {
      ParametersController.fail(res, error, 'Failed to fetch parameter notifications');
    }
  }

  private static fail(res: Response, error: unknown, fallbackMessage: string): void {
    if (error instanceof ValidationError) {
      res.status(400).json({ success: false, error: error.message });
      return;
    }
    if (error instanceof NotFoundError) {
      res.status(404).json({ success: false, error: error.message });
      return;
    }
    logger.error(fallbackMessage, error);
    res.status(500).json({ success: false, error: fallbackMessage });
  }
}

export const parametersController = new ParametersController();
