import {
  Account,
  Address,
  Contract,
  Keypair,
  Networks,
  SorobanDataBuilder,
  StrKey,
  TransactionBuilder,
  nativeToScVal,
  xdr,
} from '@stellar/stellar-sdk';
import { config } from '../config';
import { prefetchService } from '../services/prefetch.service';
import { readCacheService } from '../services/readCache.service';
import { redisCacheService } from '../services/redisCache.service';
import { StellarService } from '../services/stellar.service';
import {
  MAX_TRANSACTION_XDR_LENGTH,
  TransactionSimulationService,
  normalizeSimulation,
  parseSimulationRequest,
  toJsonSafe,
} from '../services/transactionSimulation.service';
import { ApiError, ValidationError } from '../utils/errors';

const mockSimulate = jest.fn();
jest.mock('@stellar/stellar-sdk/rpc', () => ({
  Server: jest.fn().mockImplementation(() => ({
    // Resolved lazily: the factory is hoisted above the mockSimulate declaration.
    simulateTransaction: (...args: unknown[]) => mockSimulate(...args),
  })),
}));

const USER = Keypair.random().publicKey();
const CONTRACT_ID = StrKey.encodeContract(Buffer.alloc(32, 7));
const ASSET_ID = StrKey.encodeContract(Buffer.alloc(32, 9));

function buildTx(sequence = '1') {
  const contract = new Contract(CONTRACT_ID);
  return new TransactionBuilder(new Account(USER, sequence), {
    fee: '100',
    networkPassphrase: config.stellar.networkPassphrase,
  })
    .addOperation(
      contract.call(
        'deposit_collateral',
        new Address(USER).toScVal(),
        xdr.ScVal.scvVoid(),
        nativeToScVal(BigInt(5000000), { type: 'i128' })
      )
    )
    .setTimeout(300)
    .build();
}

function sorobanData() {
  return new SorobanDataBuilder()
    .setResources(250000, 1024, 256)
    .setResourceFee(40000)
    .setReadOnly([
      xdr.LedgerKey.account(
        new xdr.LedgerKeyAccount({ accountId: Keypair.random().xdrAccountId() })
      ),
    ]);
}

function sourceAuthEntry() {
  return new xdr.SorobanAuthorizationEntry({
    credentials: xdr.SorobanCredentials.sorobanCredentialsSourceAccount(),
    rootInvocation: new xdr.SorobanAuthorizedInvocation({
      function: xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(
        new xdr.InvokeContractArgs({
          contractAddress: new Address(CONTRACT_ID).toScAddress(),
          functionName: 'deposit_collateral',
          args: [],
        })
      ),
      subInvocations: [],
    }),
  });
}

function addressAuthEntry(address: string) {
  return new xdr.SorobanAuthorizationEntry({
    credentials: xdr.SorobanCredentials.sorobanCredentialsAddress(
      new xdr.SorobanAddressCredentials({
        address: new Address(address).toScAddress(),
        nonce: xdr.Int64.fromString('1'),
        signatureExpirationLedger: 100,
        signature: xdr.ScVal.scvVoid(),
      })
    ),
    rootInvocation: sourceAuthEntry().rootInvocation(),
  });
}

function successResponse(overrides: Record<string, unknown> = {}) {
  return {
    id: '1',
    latestLedger: 4242,
    events: [],
    _parsed: true,
    transactionData: sorobanData(),
    minResourceFee: '40100',
    result: {
      auth: [sourceAuthEntry(), addressAuthEntry(USER)],
      retval: nativeToScVal(BigInt('123456789012345678901'), { type: 'i128' }),
    },
    stateChanges: [
      {
        type: 2,
        key: xdr.LedgerKey.account(
          new xdr.LedgerKeyAccount({ accountId: Keypair.random().xdrAccountId() })
        ),
        before: null,
        after: null,
      },
    ],
    ...overrides,
  };
}

describe('parseSimulationRequest', () => {
  it('accepts a transaction envelope and trims it', () => {
    expect(parseSimulationRequest({ transactionXdr: '  AAAA  ' })).toEqual({
      transactionXdr: 'AAAA',
    });
  });

  it('prefers the envelope over operation fields', () => {
    expect(parseSimulationRequest({ transactionXdr: 'AAAA', operation: 'bogus' })).toEqual({
      transactionXdr: 'AAAA',
    });
  });

  it('accepts a lending operation specification', () => {
    expect(
      parseSimulationRequest({
        operation: 'borrow',
        userAddress: USER,
        amount: '10',
        assetAddress: ASSET_ID,
      })
    ).toEqual({ operation: 'borrow', userAddress: USER, amount: '10', assetAddress: ASSET_ID });
    expect(
      parseSimulationRequest({ operation: 'deposit', userAddress: USER, amount: '10' })
    ).toEqual({
      operation: 'deposit',
      userAddress: USER,
      amount: '10',
    });
  });

  it.each([
    [undefined, 'Request body'],
    [{}, 'Provide transactionXdr'],
    [{ transactionXdr: '' }, 'transactionXdr'],
    [{ transactionXdr: 42 }, 'transactionXdr'],
    [{ transactionXdr: 'A'.repeat(MAX_TRANSACTION_XDR_LENGTH + 1) }, 'transactionXdr'],
    [{ operation: 'liquidate', userAddress: USER, amount: '10' }, 'operation'],
    [{ operation: 'deposit', userAddress: 'not-a-key', amount: '10' }, 'userAddress'],
    [{ operation: 'deposit', userAddress: USER, amount: '0' }, 'amount'],
    [{ operation: 'deposit', userAddress: USER, amount: 10 }, 'amount'],
    [{ operation: 'deposit', userAddress: USER, amount: '1.5' }, 'amount'],
    [{ operation: 'deposit', userAddress: USER, amount: '10', assetAddress: USER }, 'assetAddress'],
  ])('rejects %j', (body, message) => {
    expect(() => parseSimulationRequest(body)).toThrow(ValidationError);
    expect(() => parseSimulationRequest(body)).toThrow(message);
  });
});

describe('toJsonSafe', () => {
  it('converts bigint, bytes and maps into JSON-friendly values', () => {
    const value = toJsonSafe({
      big: 10n,
      bytes: Buffer.from('hi'),
      map: new Map([['k', 1n]]),
      list: [1n, 'x'],
      nested: { inner: 2n },
    });
    expect(value).toEqual({
      big: '10',
      bytes: 'aGk=',
      map: { k: '1' },
      list: ['1', 'x'],
      nested: { inner: '2' },
    });
    expect(JSON.stringify(value)).toBeTruthy();
  });
});

describe('normalizeSimulation', () => {
  const tx = buildTx();
  const envelope = tx.toXDR();

  it('maps a successful parsed response into resources, fees, auth and state changes', () => {
    const result = normalizeSimulation(successResponse(), tx, envelope, '2026-09-29T00:00:00.000Z');

    expect(result).toMatchObject({
      success: true,
      status: 'success',
      transactionXdr: envelope,
      sourceAccount: USER,
      operationCount: 1,
      latestLedger: 4242,
      minResourceFee: '40100',
      resources: {
        cpuInstructions: '250000',
        readBytes: '1024',
        writeBytes: '256',
        readOnlyEntries: 1,
        readWriteEntries: 0,
        resourceFee: '40000',
      },
      memoryBytes: null,
      restorePreamble: null,
      error: null,
      cached: false,
      simulatedAt: '2026-09-29T00:00:00.000Z',
    });
    expect(result.result?.retval).toBe('123456789012345678901');
    expect(result.result?.auth).toEqual([
      { xdr: expect.any(String), credentials: 'source_account' },
      { xdr: expect.any(String), credentials: 'address', address: USER },
    ]);
    expect(result.stateChanges).toEqual([{ type: 'updated', keyXdr: expect.any(String) }]);
    expect(() => JSON.stringify(result)).not.toThrow();
  });

  it('maps an error response and keeps diagnostic events', () => {
    const result = normalizeSimulation(
      { latestLedger: 10, error: 'HostError: Error(Contract, #3)', events: ['AAAA', 'BBBB'] },
      tx,
      envelope
    );
    expect(result).toMatchObject({
      success: false,
      status: 'error',
      error: 'HostError: Error(Contract, #3)',
      events: ['AAAA', 'BBBB'],
      minResourceFee: null,
      resources: null,
      result: null,
      restorePreamble: null,
    });
  });

  it('flags a restore preamble', () => {
    const result = normalizeSimulation(
      successResponse({
        restorePreamble: {
          minResourceFee: '777',
          transactionData: sorobanData().setResourceFee(700),
        },
      }),
      tx,
      envelope
    );
    expect(result.status).toBe('restore_required');
    expect(result.restorePreamble).toEqual({
      minResourceFee: '777',
      resources: expect.objectContaining({ resourceFee: '700' }),
    });
  });

  it('accepts the raw JSON-RPC shape with base64 fields and cost', () => {
    const data = sorobanData().build().toXDR('base64');
    const retval = nativeToScVal('ok').toXDR('base64');
    const result = normalizeSimulation(
      {
        latestLedger: '99',
        transactionData: data,
        minResourceFee: 500,
        cost: { cpuInsns: '1', memBytes: '2048' },
        results: [{ xdr: retval, auth: [sourceAuthEntry().toXDR('base64')] }],
        stateChanges: [{ type: 'created', key: 'AAAA', before: null, after: null }],
      },
      tx,
      envelope
    );
    expect(result).toMatchObject({
      status: 'success',
      latestLedger: 99,
      minResourceFee: '500',
      memoryBytes: '2048',
      resources: expect.objectContaining({ cpuInstructions: '250000' }),
      stateChanges: [{ type: 'created', keyXdr: 'AAAA' }],
    });
    expect(result.result).toEqual({
      retvalXdr: retval,
      retval: 'ok',
      auth: [{ xdr: expect.any(String), credentials: 'source_account' }],
    });
  });

  it('returns no invocation result when the simulation had none', () => {
    const result = normalizeSimulation(successResponse({ result: undefined }), tx, envelope);
    expect(result.result).toBeNull();
    expect(result.success).toBe(true);
  });
});

describe('TransactionSimulationService', () => {
  const originalContractId = config.stellar.contractId;
  let service: TransactionSimulationService;

  beforeEach(() => {
    config.stellar.contractId = CONTRACT_ID;
    mockSimulate.mockReset();
    redisCacheService.clearAllForTests();
    readCacheService.resetForTests();
    prefetchService.resetForTests();
    service = new TransactionSimulationService();
  });

  afterEach(() => {
    config.stellar.contractId = originalContractId;
    jest.restoreAllMocks();
  });

  it('simulates an envelope and caches the result by envelope hash', async () => {
    mockSimulate.mockResolvedValue(successResponse());
    const tx = buildTx();

    const first = await service.simulate({ transactionXdr: tx.toXDR() });
    const second = await service.simulate({ transactionXdr: tx.toXDR() });
    const other = await service.simulate({ transactionXdr: buildTx('2').toXDR() });

    expect(first).toMatchObject({ status: 'success', cached: false, sourceAccount: USER });
    expect(second).toMatchObject({ status: 'success', cached: true, minResourceFee: '40100' });
    expect(other.cached).toBe(false);
    expect(mockSimulate).toHaveBeenCalledTimes(2);
    expect(readCacheService.getStats().kinds.simulation).toMatchObject({ hits: 1, misses: 2 });
  });

  it('rejects an envelope that does not parse', async () => {
    await expect(service.simulate({ transactionXdr: 'definitely-not-xdr' })).rejects.toThrow(
      ValidationError
    );
    expect(mockSimulate).not.toHaveBeenCalled();
  });

  it('returns a status of error for failed simulations instead of throwing', async () => {
    mockSimulate.mockResolvedValue({
      latestLedger: 1,
      error: 'HostError: Error(Contract, #7)',
      events: [],
    });
    const result = await service.simulate({ transactionXdr: buildTx().toXDR() });
    expect(result).toMatchObject({
      success: false,
      status: 'error',
      error: 'HostError: Error(Contract, #7)',
    });
  });

  it('wraps RPC transport failures in a 502 ApiError and caches nothing', async () => {
    mockSimulate.mockRejectedValue(new Error('ECONNREFUSED'));
    const attempt = service.simulate({ transactionXdr: buildTx().toXDR() });
    await expect(attempt).rejects.toBeInstanceOf(ApiError);
    await expect(attempt).rejects.toMatchObject({ statusCode: 502 });
    expect(readCacheService.getStats().kinds.simulation).toMatchObject({
      stores: 0,
      loadErrors: 1,
    });
  });

  it('builds a lending operation for the user before simulating it', async () => {
    const getAccount = jest
      .spyOn(StellarService.prototype, 'getAccount')
      .mockResolvedValue(new Account(USER, '41'));
    mockSimulate.mockResolvedValue(successResponse());

    const result = await service.simulate({
      operation: 'withdraw',
      userAddress: USER,
      amount: '2500000',
      assetAddress: ASSET_ID,
    });

    expect(getAccount).toHaveBeenCalledWith(USER);
    expect(result.status).toBe('success');
    const simulated = mockSimulate.mock.calls[0][0];
    expect(simulated.source).toBe(USER);
    expect(simulated.sequence).toBe('42');
    const op = simulated.operations[0];
    expect(op.type).toBe('invokeHostFunction');
    expect(op.func.invokeContract().functionName().toString()).toBe('withdraw_collateral');
    expect(result.transactionXdr).toBe(simulated.toXDR());
  });
});
