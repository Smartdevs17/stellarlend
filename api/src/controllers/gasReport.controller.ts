import { NextFunction, Request, Response } from 'express';
import { getGasReport, renderGasReportMarkdown, OPERATION_TYPES } from '../services/gasReport';
import { NotFoundError } from '../utils/errors';

const refresh = (req: Request) => req.query.refresh === 'true';

export class GasReportController {
  getReport(req: Request, res: Response, next: NextFunction): void {
    try {
      const report = getGasReport({ refresh: refresh(req) });
      if (req.query.format === 'markdown') {
        res.type('text/markdown').send(renderGasReportMarkdown(report));
        return;
      }
      res.json(report);
    } catch (error) {
      next(error);
    }
  }

  getBudgets(req: Request, res: Response, next: NextFunction): void {
    try {
      const report = getGasReport({ refresh: refresh(req) });
      res.json({
        thresholds: report.thresholds,
        byOperationType: report.byOperationType,
        functions: report.functions.map(
          ({
            key,
            operationType,
            cpuInstructions,
            budget,
            budgetSource,
            utilizationPct,
            status,
          }) => ({
            key,
            operationType,
            cpuInstructions,
            budget,
            budgetSource,
            utilizationPct,
            status,
          })
        ),
      });
    } catch (error) {
      next(error);
    }
  }

  getBudgetForType(req: Request, res: Response, next: NextFunction): void {
    try {
      const type = req.params.type as (typeof OPERATION_TYPES)[number];
      if (!OPERATION_TYPES.includes(type)) {
        throw new NotFoundError(
          `unknown operation type; expected one of ${OPERATION_TYPES.join(', ')}`
        );
      }
      const report = getGasReport({ refresh: refresh(req) });
      res.json({
        type,
        ...report.byOperationType[type],
        functions: report.functions.filter((f) => f.operationType === type),
      });
    } catch (error) {
      next(error);
    }
  }

  getRegressions(req: Request, res: Response, next: NextFunction): void {
    try {
      const { thresholds, regressions, improvements, source } = getGasReport({
        refresh: refresh(req),
      });
      res.json({ source, thresholds, regressions, improvements });
    } catch (error) {
      next(error);
    }
  }

  getRecommendations(req: Request, res: Response, next: NextFunction): void {
    try {
      const { recommendations } = getGasReport({ refresh: refresh(req) });
      const severity = typeof req.query.severity === 'string' ? req.query.severity : undefined;
      res.json(severity ? recommendations.filter((r) => r.severity === severity) : recommendations);
    } catch (error) {
      next(error);
    }
  }

  getTrends(req: Request, res: Response, next: NextFunction): void {
    try {
      const { trends, journeys } = getGasReport({ refresh: refresh(req) });
      res.json({ trends, journeys });
    } catch (error) {
      next(error);
    }
  }
}

export const gasReportController = new GasReportController();
