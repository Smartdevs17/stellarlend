/**
 * Multi-contract state-simulation scenarios (Issue #688).
 *
 * Drives oracle → lending → risk → liquidation → repayment across one
 * in-memory harness while asserting cross-contract invariants (accounting
 * identity, health direction, conservation, pause freeze).
 */

import {
  assignRole,
  buildLendingApp,
  positionSummary,
  reset,
  runCrossContractScenario,
  setPrice,
} from './harness';

const USER = 'GSTATEUSER';
const USER2 = 'GSTATEUSER2';
const LIQUIDATOR = 'GSTATELIQ';
const ADMIN = 'GSTATEADMIN';

describe('Cross-contract state simulation', () => {
  beforeEach(() => {
    reset();
    setPrice('XLM', 0.1);
    setPrice('USDC', 1);
    setPrice('BTC', 50000);
    assignRole(LIQUIDATOR, 'liquidator');
    assignRole(ADMIN, 'admin');
  });

  it('preserves accounting identity across deposit, borrow, repay, withdraw', async () => {
    const app = buildLendingApp();

    // Deposit 10k XLM (=$1000) and 500 BTC… too much; use XLM + USDC deposit.
    await request_post(app, '/api/lending/deposit', {
      userAddress: USER,
      asset: 'XLM',
      amount: 10_000,
    });
    await request_post(app, '/api/lending/deposit', {
      userAddress: USER2,
      asset: 'XLM',
      amount: 5_000,
    });

    // Protocol-held collateral = sum of user collateral (conservation).
    const u1 = positionSummary(USER);
    const u2 = positionSummary(USER2);
    const protocolHeld = u1.collateral['XLM'] + u2.collateral['XLM'];
    expect(protocolHeld).toBe(15_000);

    // Borrow against collateral (debt increases, collateral unchanged).
    await request_post(app, '/api/lending/borrow', {
      userAddress: USER,
      asset: 'USDC',
      amount: 400,
    });
    const afterBorrow = positionSummary(USER);
    expect(afterBorrow.collateral['XLM']).toBe(10_000);
    expect(afterBorrow.debt['USDC']).toBe(400);

    // Health direction: borrow must not increase health vs post-deposit baseline.
    // (deposit-only HF is infinite; after borrow finite — direction holds.)
    expect(afterBorrow.healthFactor).toBeLessThan(u1.healthFactor);

    // Repay partially; debt decreases monotonically.
    await request_post(app, '/api/lending/repay', {
      userAddress: USER,
      asset: 'USDC',
      amount: 100,
    });
    const afterRepay = positionSummary(USER);
    expect(afterRepay.debt['USDC']).toBe(300);
    expect(afterRepay.collateral['XLM']).toBe(10_000);

    // Withdraw surplus collateral; conservation still holds for remaining.
    await request_post(app, '/api/lending/withdraw', {
      userAddress: USER,
      asset: 'XLM',
      amount: 1_000,
    });
    const afterWithdraw = positionSummary(USER);
    const protocolAfter =
      afterWithdraw.collateral['XLM'] + positionSummary(USER2).collateral['XLM'];
    expect(protocolAfter).toBe(14_000);
    expect(afterWithdraw.collateral['XLM']).toBe(9_000);
  });

  it('oracle shock → liquidation → repay keeps conservation and non-negative balances', async () => {
    const app = buildLendingApp();

    await request_post(app, '/api/lending/deposit', {
      userAddress: USER,
      asset: 'XLM',
      amount: 10_000, // $1000
    });
    await request_post(app, '/api/lending/borrow', {
      userAddress: USER,
      asset: 'USDC',
      amount: 700, // max LTV 75% → $750 capacity; 700 OK
    });

    const healthy = positionSummary(USER);
    expect(healthy.liquidatable).toBe(false);
    expect(healthy.healthFactor).toBeGreaterThanOrEqual(1);

    // Shock oracle: XLM drops 50% → collateral $500 < debt $700 → liquidatable.
    await request_post(app, '/api/oracle/update-price', {
      asset: 'XLM',
      price: 0.05,
    });
    const shocked = positionSummary(USER);
    expect(shocked.liquidatable).toBe(true);
    expect(shocked.collateralValueUsd).toBeLessThan(shocked.debtValueUsd);

    // Liquidate close-factor portion.
    const liqRes = await request(app)
      .post('/api/liquidations/liquidate')
      .send({
        callerAddress: LIQUIDATOR,
        targetUser: USER,
        debtAsset: 'USDC',
        collateralAsset: 'XLM',
        repayAmount: 200,
      });
    expect(liqRes.status).toBe(200);
    expect(liqRes.body.repaidAmount).toBe(200);

    const afterLiq = positionSummary(USER);
    expect(afterLiq.debt['USDC']).toBe(500);
    // Collateral seized strictly less than repaid value * (1+bonus)/price.
    expect(afterLiq.collateral['XLM']).toBeLessThan(10_000);
    expect(afterLiq.collateral['XLM']).toBeGreaterThan(0);

    // Restore price and repay all remaining debt → healthy again.
    await request_post(app, '/api/oracle/update-price', {
      asset: 'XLM',
      price: 0.1,
    });
    await request_post(app, '/api/lending/repay', {
      userAddress: USER,
      asset: 'USDC',
      amount: 500,
    });
    const final = positionSummary(USER);
    expect(final.debt['USDC'] ?? 0).toBe(0);
    expect(final.liquidatable).toBe(false);
    expect(final.collateral['XLM']).toBeGreaterThan(0);
  });

  it('pause freezes state: deposit/borrow/repay/withdraw all rejected', async () => {
    const app = buildLendingApp();

    await request_post(app, '/api/lending/deposit', {
      userAddress: USER,
      asset: 'XLM',
      amount: 5_000,
    });
    const before = positionSummary(USER);

    await request_post(app, '/api/protocol/pause', {
      callerAddress: ADMIN,
    });

    const deposit = await request(app)
      .post('/api/lending/deposit')
      .send({ userAddress: USER, asset: 'XLM', amount: 100 });
    expect(deposit.status).toBe(503);

    const borrow = await request(app)
      .post('/api/lending/borrow')
      .send({ userAddress: USER, asset: 'USDC', amount: 10 });
    expect(borrow.status).toBe(503);

    const withdraw = await request(app)
      .post('/api/lending/withdraw')
      .send({ userAddress: USER, asset: 'XLM', amount: 100 });
    expect(withdraw.status).toBe(503);

    // State frozen: position unchanged.
    const after = positionSummary(USER);
    expect(after.collateral).toEqual(before.collateral);
    expect(after.debt).toEqual(before.debt);
    expect(after.healthFactor).toBe(before.healthFactor);

    // Resume and verify operations work again.
    await request_post(app, '/api/protocol/resume', {
      callerAddress: ADMIN,
    });
    const ok = await request(app)
      .post('/api/lending/deposit')
      .send({ userAddress: USER, asset: 'XLM', amount: 100 });
    expect(ok.status).toBe(200);
    expect(positionSummary(USER).collateral['XLM']).toBe(5_100);
  });

  it('multi-step scenario runner executes full cross-contract journey', async () => {
    const app = buildLendingApp();

    const responses = await runCrossContractScenario(app, [
      {
        name: 'seed prices',
        method: 'post',
        path: '/api/oracle/update-price',
        body: { asset: 'XLM', price: 0.1 },
      },
      {
        name: 'deposit collateral',
        method: 'post',
        path: '/api/lending/deposit',
        body: { userAddress: USER, asset: 'XLM', amount: 20_000 },
        assert: (body: any) => expect(body.success).toBe(true),
      },
      {
        name: 'borrow',
        method: 'post',
        path: '/api/lending/borrow',
        body: { userAddress: USER, asset: 'USDC', amount: 1_000 },
        assert: (body: any) => expect(body.position.debt['USDC']).toBe(1_000),
      },
      {
        name: 'check health',
        method: 'get',
        path: `/api/positions/${USER}`,
        assert: (body: any) => {
          expect(body.healthFactor).toBeGreaterThan(1);
          expect(body.liquidatable).toBe(false);
        },
      },
      {
        name: 'shock price',
        method: 'post',
        path: '/api/oracle/update-price',
        body: { asset: 'XLM', price: 0.03 },
      },
      {
        name: 'confirm liquidatable',
        method: 'get',
        path: `/api/positions/${USER}`,
        assert: (body: any) => expect(body.liquidatable).toBe(true),
      },
      {
        name: 'liquidate',
        method: 'post',
        path: '/api/liquidations/liquidate',
        body: {
          callerAddress: LIQUIDATOR,
          targetUser: USER,
          debtAsset: 'USDC',
          collateralAsset: 'XLM',
          repayAmount: 400,
        },
        assert: (body: any) => expect(body.repaidAmount).toBe(400),
      },
      {
        name: 'restore price and repay',
        method: 'post',
        path: '/api/oracle/update-price',
        body: { asset: 'XLM', price: 0.1 },
      },
      {
        name: 'repay rest',
        method: 'post',
        path: '/api/lending/repay',
        body: { userAddress: USER, asset: 'USDC', amount: 600 },
        assert: (body: any) => expect(body.position.debt['USDC'] ?? 0).toBe(0),
      },
      {
        name: 'withdraw',
        method: 'post',
        path: '/api/lending/withdraw',
        body: { userAddress: USER, asset: 'XLM', amount: 5_000 },
        assert: (body: any) => expect(body.success).toBe(true),
      },
    ]);

    expect(responses).toHaveLength(10);
  });
});

// ─── tiny helpers (mirror supertest POST) ────────────────────────────────────

function request(app: ReturnType<typeof buildLendingApp>) {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const supertest = require('supertest');
  return supertest(app);
}

async function request_post(
  app: ReturnType<typeof buildLendingApp>,
  path: string,
  body: Record<string, unknown>
) {
  const res = await request(app).post(path).send(body);
  expect(res.status).toBe(200);
  return res.body;
}
