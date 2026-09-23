import { Request, Response } from 'express';
import { insuranceService } from '../services/insurance/insurance.service';
import { AuthRequest } from '../middleware/auth';
import { UnauthorizedError } from '../utils/errors';

export class InsuranceController {
  listPolicies(_req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.listPolicies() });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  onboardProvider(req: AuthRequest, res: Response) {
    try {
      const userAddress = req.user?.address;
      if (!userAddress) {
        throw new UnauthorizedError('Authentication required');
      }
      // Force kycStatus to 'pending' on self-onboarding; bind address to authenticated caller
      const provider = insuranceService.onboardProvider({
        ...req.body,
        address: userAddress,
        kycStatus: 'pending',
      });
      res.status(201).json({ success: true, data: provider });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  updateKycStatus(req: Request, res: Response) {
    try {
      const { id } = req.params!;
      const { status } = req.body;
      if (!['pending', 'approved', 'rejected'].includes(status)) {
        res.status(400).json({ success: false, error: 'status must be pending, approved, or rejected' });
        return;
      }
      const provider = insuranceService.updateKycStatus(id!, status);
      res.status(200).json({ success: true, data: provider });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  createPolicy(req: AuthRequest, res: Response) {
    try {
      const userAddress = req.user?.address;
      if (!userAddress) {
        throw new UnauthorizedError('Authentication required');
      }
      const policy = insuranceService.createPolicy(req.body, userAddress);
      res.status(201).json({ success: true, data: policy });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  purchase(req: AuthRequest, res: Response) {
    try {
      const userAddress = req.user?.address;
      if (!userAddress) {
        throw new UnauthorizedError('Authentication required');
      }
      // Bind coverage purchase to authenticated lender
      const coverage = insuranceService.purchase(
        req.body.policyId,
        userAddress,
        req.body.positionId,
        req.body.coverageAmount
      );
      res.status(201).json({ success: true, data: coverage });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  submitClaim(req: AuthRequest, res: Response) {
    try {
      const userAddress = req.user?.address;
      if (!userAddress) {
        throw new UnauthorizedError('Authentication required');
      }
      const claim = insuranceService.submitClaim(
        req.body.coverageId,
        req.body.trigger,
        req.body.evidence,
        Number(req.body.amount),
        userAddress
      );
      res.status(201).json({ success: true, data: claim });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  disputeClaim(req: AuthRequest, res: Response) {
    try {
      const userAddress = req.user?.address;
      if (!userAddress) {
        throw new UnauthorizedError('Authentication required');
      }
      const claim = insuranceService.disputeClaim(req.params.id!, userAddress);
      res.status(200).json({ success: true, data: claim });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  getClaims(req: Request, res: Response) {
    try {
      res.json({
        success: true,
        data: insuranceService.dashboard(req.query.providerId as string | undefined),
      });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
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
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  listProviders(_req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.listProviders() });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  getAnalytics(_req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.getAnalytics() });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }

  listCoverages(req: Request, res: Response) {
    try {
      res.json({ success: true, data: insuranceService.listCoverages(req.query.lender as string) });
    } catch (error) {
      const status = (error as any).statusCode || 400;
      res.status(status).json({ success: false, error: error instanceof Error ? error.message : 'Invalid request' });
    }
  }
}

export const insuranceController = new InsuranceController();
