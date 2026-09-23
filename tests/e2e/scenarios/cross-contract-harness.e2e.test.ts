import {
  assignRole,
  buildLendingApp,
  reset,
  runCrossContractScenario,
  setPrice,
} from './harness';

const USER = 'GUSER';
const LIQUIDATOR = 'GLIQUIDATOR';

describe('Cross-contract integration harness', () => {
  beforeEach(() => {
    reset();
    setPrice('XLM', 0.1);
    setPrice('USDC', 1);
    assignRole(LIQUIDATOR, 'liquidator');
  });

  it('drives oracle, lending, risk, liquidation, repay, and withdraw steps through one scenario', async () => {
    const app = buildLendingApp();

    const responses = await runCrossContractScenario(app, [
      {
        name: 'update oracle price',
        method: 'post',
        path: '/api/oracle/update-price',
        body: { asset: 'XLM', price: 0.08 },
        assert: (body) => expect(body).toMatchObject({ success: true, asset: 'XLM' }),
      },
      {
        name: 'deposit collateral',
        method: 'post',
        path: '/api/lending/deposit',
        body: { userAddress: USER, asset: 'XLM', amount: 10_000 },
        assert: (body: any) => expect(body.position.collateralValueUsd).toBeCloseTo(800),
      },
      {
        name: 'borrow against collateral',
        method: 'post',
        path: '/api/lending/borrow',
        body: { userAddress: USER, asset: 'USDC', amount: 500 },
        assert: (body: any) => expect(body.position.debtValueUsd).toBe(500),
      },
      {
        name: 'oracle shock makes position liquidatable',
        method: 'post',
        path: '/api/oracle/update-price',
        body: { asset: 'XLM', price: 0.05 },
      },
      {
        name: 'read risk state',
        method: 'get',
        path: `/api/positions/${USER}`,
        assert: (body: any) => expect(body.liquidatable).toBe(true),
      },
      {
        name: 'liquidate unhealthy position',
        method: 'post',
        path: '/api/liquidations/liquidate',
        body: {
          callerAddress: LIQUIDATOR,
          targetUser: USER,
          debtAsset: 'USDC',
          collateralAsset: 'XLM',
          repayAmount: 100,
        },
        assert: (body: any) => expect(body.repaidAmount).toBe(100),
      },
      {
        name: 'repay remaining debt',
        method: 'post',
        path: '/api/lending/repay',
        body: { userAddress: USER, asset: 'USDC', amount: 400 },
      },
      {
        name: 'withdraw after solvency restored',
        method: 'post',
        path: '/api/lending/withdraw',
        body: { userAddress: USER, asset: 'XLM', amount: 1000 },
        assert: (body: any) => expect(body.success).toBe(true),
      },
    ]);

    expect(responses).toHaveLength(8);
  });
});
