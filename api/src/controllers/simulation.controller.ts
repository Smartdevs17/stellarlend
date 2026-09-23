import { Request, Response, NextFunction } from 'express';
import { positionSimulator } from '../services/position-simulator';
import logger from '../utils/logger';
import crypto from 'crypto';

export interface ScenarioParams {
  priceChangePercent?: number;
  depositAmount?: number;
  withdrawAmount?: number;
  borrowAmount?: number;
  repayAmount?: number;
  scenarioName?: string;
}

export interface SharedSimulationRecord {
  id: string;
  position: {
    collateral: number;
    debt: number;
    asset: string;
  };
  scenario: ScenarioParams;
  result: any;
  createdAt: number;
  expiresAt: number;
  createdBy?: string;
}

const sharedSimulations = new Map<string, SharedSimulationRecord>();

export class SimulationController {
  /**
   * Simulate position health under user-specified price changes, deposits, and withdrawals.
   * POST /api/simulation/position
   */
  async simulatePosition(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { position, scenario } = req.body;
      if (!position || typeof position.collateral !== 'number' || typeof position.debt !== 'number') {
        res.status(400).json({
          success: false,
          error: 'Valid position with numeric collateral and debt is required',
        });
        return;
      }

      const params: ScenarioParams = scenario || {};
      const priceChange = params.priceChangePercent ?? 0;
      const deposit = params.depositAmount ?? 0;
      const withdraw = params.withdrawAmount ?? 0;
      const borrow = params.borrowAmount ?? 0;
      const repay = params.repayAmount ?? 0;

      // Base health
      const initialHealth = position.debt === 0 ? Infinity : position.collateral / position.debt;

      // Effective collateral after deposit/withdrawal and price impact
      const collateralAfterOps = Math.max(0, position.collateral + deposit - withdraw);
      const simulatedCollateral = Math.max(0, collateralAfterOps * (1 + priceChange / 100));

      // Effective debt after borrow/repayment
      const simulatedDebt = Math.max(0, position.debt + borrow - repay);

      const simulatedHealth = simulatedDebt === 0 ? Infinity : simulatedCollateral / simulatedDebt;
      const isLiquidatable = simulatedHealth < 1.0;

      // Liquidation price (when collateralValue == debt)
      const liquidationPrice = position.collateral === 0 ? 0 : position.debt / position.collateral;
      const liquidationPriceDropPercent =
        position.collateral <= position.debt
          ? 0
          : ((position.collateral - position.debt) / position.collateral) * 100;

      // Max safe amounts (keeping health >= 1.0)
      const maxWithdrawable = Math.max(0, simulatedCollateral - simulatedDebt);
      const maxBorrowable = Math.max(0, simulatedCollateral - simulatedDebt);

      const result = {
        scenario_name: params.scenarioName || 'Custom Scenario',
        initial_position: {
          collateral: position.collateral,
          debt: position.debt,
          asset: position.asset || 'USDC',
          health_factor: Number.isFinite(initialHealth) ? Math.round(initialHealth * 100) / 100 : 9999,
        },
        simulated_position: {
          collateral: Math.round(simulatedCollateral * 100) / 100,
          debt: Math.round(simulatedDebt * 100) / 100,
          health_factor: Number.isFinite(simulatedHealth) ? Math.round(simulatedHealth * 100) / 100 : 9999,
        },
        is_liquidatable: isLiquidatable,
        liquidation_price: Math.round(liquidationPrice * 10000) / 10000,
        liquidation_price_drop_percent: Math.round(liquidationPriceDropPercent * 100) / 100,
        max_withdrawable_amount: Math.round(maxWithdrawable * 100) / 100,
        max_borrowable_amount: Math.round(maxBorrowable * 100) / 100,
        safety_margin_percent: Number.isFinite(simulatedHealth)
          ? Math.round((simulatedHealth - 1.0) * 10000) / 100
          : 9999,
        timestamp: Date.now(),
      };

      res.status(200).json({ success: true, data: result });
    } catch (error) {
      logger.error('Error simulating position:', error);
      next(error);
    }
  }

  /**
   * Run scenario modeling across multiple pre-defined and stress-test scenarios.
   * POST /api/simulation/scenario
   */
  async simulateScenario(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { position, scenarios } = req.body;
      if (!position || typeof position.collateral !== 'number' || typeof position.debt !== 'number') {
        res.status(400).json({
          success: false,
          error: 'Valid position with numeric collateral and debt is required',
        });
        return;
      }

      const defaultScenarios: ScenarioParams[] = scenarios || [
        { scenarioName: 'Mild Market Dip (-10%)', priceChangePercent: -10 },
        { scenarioName: 'Moderate Correction (-25%)', priceChangePercent: -25 },
        { scenarioName: 'Severe Flash Crash (-40%)', priceChangePercent: -40 },
        { scenarioName: 'Black Swan Event (-60%)', priceChangePercent: -60 },
        { scenarioName: 'Market Recovery (+20%)', priceChangePercent: 20 },
        { scenarioName: 'Defensive Collateral Top-Up (+25% deposit)', depositAmount: position.collateral * 0.25 },
        { scenarioName: 'Partial De-leverage (-50% debt repay)', repayAmount: position.debt * 0.5 },
      ];

      const results = defaultScenarios.map((sc) => {
        const priceChange = sc.priceChangePercent ?? 0;
        const deposit = sc.depositAmount ?? 0;
        const withdraw = sc.withdrawAmount ?? 0;
        const borrow = sc.borrowAmount ?? 0;
        const repay = sc.repayAmount ?? 0;

        const initialHealth = position.debt === 0 ? 9999 : position.collateral / position.debt;
        const collateralAfterOps = Math.max(0, position.collateral + deposit - withdraw);
        const simulatedCollateral = Math.max(0, collateralAfterOps * (1 + priceChange / 100));
        const simulatedDebt = Math.max(0, position.debt + borrow - repay);
        const finalHealth = simulatedDebt === 0 ? 9999 : simulatedCollateral / simulatedDebt;

        return {
          scenario_name: sc.scenarioName || 'Unnamed Scenario',
          price_change_percent: priceChange,
          simulated_collateral: Math.round(simulatedCollateral * 100) / 100,
          simulated_debt: Math.round(simulatedDebt * 100) / 100,
          initial_health: Math.round(initialHealth * 100) / 100,
          final_health: Math.round(finalHealth * 100) / 100,
          is_liquidatable: finalHealth < 1.0,
          safety_margin: Math.round((finalHealth - 1.0) * 100) / 100,
        };
      });

      res.status(200).json({ success: true, data: { position, results } });
    } catch (error) {
      logger.error('Error running scenario modeling:', error);
      next(error);
    }
  }

  /**
   * What-if analysis: calculate exact threshold sensitivities and action recommendations.
   * POST /api/simulation/what-if
   */
  async whatIfAnalysis(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { position, targetHealthFactor = 1.5 } = req.body;
      if (!position || typeof position.collateral !== 'number' || typeof position.debt !== 'number') {
        res.status(400).json({
          success: false,
          error: 'Valid position with numeric collateral and debt is required',
        });
        return;
      }

      const currentHealth = position.debt === 0 ? 9999 : position.collateral / position.debt;
      const target = Number(targetHealthFactor) || 1.5;

      // Required deposit to achieve target health factor
      // (collateral + neededDeposit) / debt = target => neededDeposit = target * debt - collateral
      const requiredDepositForTarget = Math.max(0, target * position.debt - position.collateral);

      // Required debt repayment to achieve target health factor
      // collateral / (debt - neededRepay) = target => debt - neededRepay = collateral / target => neededRepay = debt - collateral / target
      const requiredRepaymentForTarget = Math.max(0, position.debt - position.collateral / target);

      // Max borrow keeping health >= target
      // (collateral) / (debt + additionalBorrow) = target => debt + additionalBorrow = collateral / target
      const maxBorrowAtTarget = Math.max(0, position.collateral / target - position.debt);

      // Max withdraw keeping health >= target
      // (collateral - withdraw) / debt = target => collateral - withdraw = target * debt => withdraw = collateral - target * debt
      const maxWithdrawAtTarget = Math.max(0, position.collateral - target * position.debt);

      // Liquidation threshold (target = 1.0)
      const liquidationPrice = position.collateral > 0 ? position.debt / position.collateral : 0;
      const liquidationDropPercent =
        position.collateral > position.debt
          ? ((position.collateral - position.debt) / position.collateral) * 100
          : 0;

      res.status(200).json({
        success: true,
        data: {
          current_health_factor: Math.round(currentHealth * 100) / 100,
          target_health_factor: target,
          liquidation_price: Math.round(liquidationPrice * 10000) / 10000,
          liquidation_drop_percent: Math.round(liquidationDropPercent * 100) / 100,
          required_deposit_for_target: Math.round(requiredDepositForTarget * 100) / 100,
          required_repayment_for_target: Math.round(requiredRepaymentForTarget * 100) / 100,
          max_borrow_at_target: Math.round(maxBorrowAtTarget * 100) / 100,
          max_withdraw_at_target: Math.round(maxWithdrawAtTarget * 100) / 100,
          is_currently_safe: currentHealth >= target,
        },
      });
    } catch (error) {
      logger.error('Error in what-if analysis:', error);
      next(error);
    }
  }

  /**
   * Share a simulation scenario with a persistent link/token.
   * POST /api/simulation/share
   */
  async shareSimulation(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { position, scenario, result, createdBy } = req.body;
      if (!position || !scenario) {
        res.status(400).json({
          success: false,
          error: 'Position and scenario are required to share a simulation',
        });
        return;
      }

      const id = crypto.randomBytes(8).toString('hex');
      const now = Date.now();
      const expiresAt = now + 30 * 24 * 60 * 60 * 1000; // 30 days

      const record: SharedSimulationRecord = {
        id,
        position,
        scenario,
        result: result || positionSimulator.simulatePriceDrop(position, scenario.priceChangePercent ?? 0),
        createdAt: now,
        expiresAt,
        createdBy: createdBy || 'anonymous',
      };

      sharedSimulations.set(id, record);

      res.status(201).json({
        success: true,
        data: {
          id,
          share_url: `/simulation/view/${id}`,
          share_token: id,
          expires_at: expiresAt,
          record,
        },
      });
    } catch (error) {
      logger.error('Error sharing simulation:', error);
      next(error);
    }
  }

  /**
   * Retrieve a shared simulation by ID.
   * GET /api/simulation/share/:id
   */
  async getSharedSimulation(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { id } = req.params;
      const record = sharedSimulations.get(id);

      if (!record) {
        res.status(404).json({ success: false, error: 'Shared simulation not found or expired' });
        return;
      }

      if (Date.now() > record.expiresAt) {
        sharedSimulations.delete(id);
        res.status(410).json({ success: false, error: 'Shared simulation has expired' });
        return;
      }

      res.status(200).json({ success: true, data: record });
    } catch (error) {
      logger.error('Error retrieving shared simulation:', error);
      next(error);
    }
  }

  /**
   * Compare two or more scenarios side by side.
   * POST /api/simulation/compare
   */
  async compareScenarios(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { scenarioA, scenarioB } = req.body;
      if (!scenarioA || !scenarioB) {
        res.status(400).json({ success: false, error: 'Both scenarioA and scenarioB are required' });
        return;
      }

      const comparison = positionSimulator.compareScenarios(scenarioA, scenarioB);
      res.status(200).json({ success: true, data: comparison });
    } catch (error) {
      logger.error('Error comparing scenarios:', error);
      next(error);
    }
  }

  /**
   * Batch simulate health factors across multiple pools or accounts.
   * POST /api/simulation/batch
   */
  async batchSimulate(req: Request, res: Response, next: NextFunction): Promise<void> {
    try {
      const { positions, scenario } = req.body;
      if (!Array.isArray(positions) || positions.length === 0) {
        res.status(400).json({ success: false, error: 'Array of positions is required' });
        return;
      }

      const results = positions.map((p) => {
        const priceChange = scenario?.priceChangePercent ?? 0;
        const initialHealth = p.debt === 0 ? 9999 : p.collateral / p.debt;
        const simCollateral = Math.max(0, p.collateral * (1 + priceChange / 100));
        const simHealth = p.debt === 0 ? 9999 : simCollateral / p.debt;

        return {
          asset: p.asset,
          initial_health: Math.round(initialHealth * 100) / 100,
          simulated_health: Math.round(simHealth * 100) / 100,
          is_liquidatable: simHealth < 1.0,
          liquidation_price: p.collateral > 0 ? Math.round((p.debt / p.collateral) * 10000) / 10000 : 0,
        };
      });

      res.status(200).json({ success: true, data: { results } });
    } catch (error) {
      logger.error('Error in batch simulation:', error);
      next(error);
    }
  }
}

export const simulationController = new SimulationController();
