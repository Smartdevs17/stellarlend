/**
 * Governance proposal tracking routes (Issue #1087).
 *
 * Mounted at /api/v1/governance/proposals (see v1/governance/index.ts):
 * - GET  /                        — list proposals (status/proposalType/search/limit/offset)
 * - GET  /stats/summary           — counts by status for dashboard cards
 * - GET  /:id                     — proposal detail with vote tallies
 */

import { Router, Request, Response, NextFunction } from 'express';
import { governanceProposalsService } from '../../../services/governanceProposals.service';

const router: Router = Router();

router.get('/', (req: Request, res: Response, next: NextFunction) => {
  try {
    const result = governanceProposalsService.listProposals({
      status: typeof req.query.status === 'string' ? req.query.status : undefined,
      proposalType: typeof req.query.proposalType === 'string' ? req.query.proposalType : undefined,
      search: typeof req.query.search === 'string' ? req.query.search : undefined,
      limit: req.query.limit !== undefined ? Number(req.query.limit) : undefined,
      offset: req.query.offset !== undefined ? Number(req.query.offset) : undefined,
    });
    res.json({ success: true, data: result });
  } catch (error) {
    next(error);
  }
});

router.get('/stats/summary', (_req: Request, res: Response, next: NextFunction) => {
  try {
    res.json({ success: true, data: governanceProposalsService.getStats() });
  } catch (error) {
    next(error);
  }
});

router.get('/:id', (req: Request, res: Response, next: NextFunction) => {
  try {
    const id = req.params.id;
    if (!id) {
      return res.status(400).json({ success: false, error: 'Proposal id is required' });
    }
    const proposal = governanceProposalsService.getProposal(id);
    if (!proposal) {
      return res.status(404).json({ success: false, error: 'Proposal not found' });
    }
    return res.json({ success: true, data: proposal });
  } catch (error) {
    return next(error);
  }
});

export default router;
