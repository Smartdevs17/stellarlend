import { Request, Response } from 'express';
import { insuranceService } from '../services/insurance/insurance.service';

export class InsuranceController {
  listPolicies(_req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.listPolicies() });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  onboardProvider(req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.onboardProvider(req.body) });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  createPolicy(req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.createPolicy(req.body) });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  purchase(req: Request, res: Response) {
    try {
      res.json({
        success: true,
        data: insuranceService.purchase(
          req.body.policyId,
          req.body.lender,
          req.body.positionId,
          req.body.coverageAmount
        ),
      });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  submitClaim(req: Request, res: Response) {
    try {
      res.json({
        success: true,
        data: insuranceService.submitClaim(
          req.body.coverageId,
          req.body.trigger,
          req.body.evidence,
          Number(req.body.amount)
        ),
      });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  disputeClaim(req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.disputeClaim(req.params.id!) });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  getClaims(req: Request, res: Response) {
    try {
      res.json({
        success: true,
        data: insuranceService.dashboard(req.query.providerId as string | undefined),
      });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  calculatePremium(req: Request, res: Response) {
    try {
      const { policyId, riskScore } = req.body;
      if (!policyId || riskScore === undefined) {
        res.status(400).json({ success: false, error: 'policyId and riskScore required' });
        return;
      }
      res.json({ success: true, data: insuranceService.calculatePremium(policyId, Number(riskScore)) });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  listProviders(_req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.listProviders() });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  getAnalytics(_req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.getAnalytics() });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  listCoverages(req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.listCoverages(req.query.lender as string) });
    } catch (error) {
      res.status(400).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }
}

export const insuranceController = new InsuranceController();
