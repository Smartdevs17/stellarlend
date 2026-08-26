import { Request, Response } from 'express';
import { getCurrentBorrowRate, getCurrentSupplyRate, getCurrentUtilization, getRateHistory, simulateRateAtUtilization } from '../services/interest.service';

export class InterestController {
  async getCurrentRates(_req: Request, res: Response): Promise<void> {
    try {
      const borrowRate = await getCurrentBorrowRate();
      const supplyRate = await getCurrentSupplyRate();
      const utilization = await getCurrentUtilization();
      res.json({ borrowRateBps: borrowRate, supplyRateBps: supplyRate, utilizationBps: utilization });
    } catch (error) {
      res.status(500).json({ error: 'Failed to fetch interest rates' });
    }
  }

  async getRateHistory(_req: Request, res: Response): Promise<void> {
    try {
      const history = await getRateHistory();
      res.json(history);
    } catch (error) {
      res.status(500).json({ error: 'Failed to fetch rate history' });
    }
  }

  async simulateRate(req: Request, res: Response): Promise<void> {
    try {
      const utilizationBps = Number(req.body?.utilizationBps ?? 0);
      const rate = await simulateRateAtUtilization(utilizationBps);
      res.json({ simulatedBorrowRateBps: rate });
    } catch (error) {
      res.status(500).json({ error: 'Failed to simulate rate' });
    }
  }
}

export const interestController = new InterestController();
