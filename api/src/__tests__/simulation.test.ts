import { simulationController } from '../controllers/simulation.controller';
import { Request, Response } from 'express';

describe('Simulation Controller (Issue #731)', () => {
  const mockResponse = () => {
    const res: Partial<Response> = {};
    res.status = jest.fn().mockReturnValue(res);
    res.json = jest.fn().mockReturnValue(res);
    return res as Response;
  };

  test('simulatePosition correctly calculates healthy state and liquidation price', async () => {
    const req = {
      body: {
        position: { collateral: 2000, debt: 1000, asset: 'USDC' },
        scenario: { priceChangePercent: -20, depositAmount: 0, withdrawAmount: 0 },
      },
    } as unknown as Request;
    const res = mockResponse();
    const next = jest.fn();

    await simulationController.simulatePosition(req, res, next);

    expect(res.status).toHaveBeenCalledWith(200);
    const jsonCall = (res.json as jest.Mock).mock.calls[0][0];
    expect(jsonCall.success).toBe(true);
    expect(jsonCall.data.initial_position.health_factor).toBe(2);
    // 2000 * 0.8 = 1600 / 1000 = 1.6
    expect(jsonCall.data.simulated_position.health_factor).toBe(1.6);
    expect(jsonCall.data.is_liquidatable).toBe(false);
    expect(jsonCall.data.liquidation_price).toBe(0.5);
    expect(jsonCall.data.liquidation_price_drop_percent).toBe(50);
  });

  test('simulatePosition identifies liquidatable scenario upon severe crash', async () => {
    const req = {
      body: {
        position: { collateral: 1000, debt: 800, asset: 'USDC' },
        scenario: { priceChangePercent: -40 },
      },
    } as unknown as Request;
    const res = mockResponse();
    const next = jest.fn();

    await simulationController.simulatePosition(req, res, next);

    expect(res.status).toHaveBeenCalledWith(200);
    const jsonCall = (res.json as jest.Mock).mock.calls[0][0];
    // 1000 * 0.6 = 600 / 800 = 0.75 < 1.0
    expect(jsonCall.data.simulated_position.health_factor).toBe(0.75);
    expect(jsonCall.data.is_liquidatable).toBe(true);
  });

  test('simulateScenario runs multi-scenario stress modeling', async () => {
    const req = {
      body: {
        position: { collateral: 1500, debt: 500, asset: 'USDC' },
      },
    } as unknown as Request;
    const res = mockResponse();
    const next = jest.fn();

    await simulationController.simulateScenario(req, res, next);

    expect(res.status).toHaveBeenCalledWith(200);
    const jsonCall = (res.json as jest.Mock).mock.calls[0][0];
    expect(jsonCall.success).toBe(true);
    expect(jsonCall.data.results.length).toBeGreaterThan(3);
    const mild = jsonCall.data.results.find((r: any) => r.scenario_name.includes('Mild Market Dip'));
    expect(mild).toBeDefined();
    expect(mild.is_liquidatable).toBe(false);
  });

  test('whatIfAnalysis computes action thresholds for target health', async () => {
    const req = {
      body: {
        position: { collateral: 1000, debt: 1000, asset: 'USDC' },
        targetHealthFactor: 1.5,
      },
    } as unknown as Request;
    const res = mockResponse();
    const next = jest.fn();

    await simulationController.whatIfAnalysis(req, res, next);

    expect(res.status).toHaveBeenCalledWith(200);
    const jsonCall = (res.json as jest.Mock).mock.calls[0][0];
    expect(jsonCall.success).toBe(true);
    // target is 1.5 => needed collateral = 1500 => deposit needed = 500
    expect(jsonCall.data.required_deposit_for_target).toBe(500);
    // debt needed = 1000 / 1.5 = 666.67 => repay needed = 333.33
    expect(jsonCall.data.required_repayment_for_target).toBeCloseTo(333.33, 1);
  });

  test('shareSimulation stores and retrieves simulation scenario', async () => {
    const shareReq = {
      body: {
        position: { collateral: 1000, debt: 500, asset: 'USDC' },
        scenario: { priceChangePercent: -25 },
        createdBy: 'user_alice',
      },
    } as unknown as Request;
    const shareRes = mockResponse();
    const next = jest.fn();

    await simulationController.shareSimulation(shareReq, shareRes, next);
    expect(shareRes.status).toHaveBeenCalledWith(201);
    const token = (shareRes.json as jest.Mock).mock.calls[0][0].data.share_token;

    const getReq = {
      params: { id: token },
    } as unknown as Request;
    const getRes = mockResponse();

    await simulationController.getSharedSimulation(getReq, getRes, next);
    expect(getRes.status).toHaveBeenCalledWith(200);
    const fetched = (getRes.json as jest.Mock).mock.calls[0][0].data;
    expect(fetched.id).toBe(token);
    expect(fetched.createdBy).toBe('user_alice');
  });
});
