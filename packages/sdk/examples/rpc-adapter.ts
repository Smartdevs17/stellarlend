/**
 * Example: a production `ContractAdapter` backed by Soroban RPC.
 *
 * `StellarLendClient` never talks to the network itself; it hands every call to an
 * adapter. This one builds the transaction, simulates it, asks a signer (wallet or
 * server key) to sign, submits it and waits for the result.
 *
 * Argument encoding follows the lending contract's interface: `G…`/`C…` strings become
 * `Address`, `bigint` becomes `i128`, plain objects become structs (symbol-keyed maps).
 */
import {
  Account,
  Address,
  BASE_FEE,
  Contract,
  Keypair,
  StrKey,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  scValToNative,
  xdr,
} from '@stellar/stellar-sdk';
import type { ContractAdapter, ContractInvocation, ContractValue } from '../src';

/** Signs a base64 transaction envelope and returns the signed envelope. */
export type TransactionSigner = (
  txXdr: string,
  options: { networkPassphrase: string },
) => Promise<string>;

/** The subset of `rpc.Server` the adapter uses (lets tests pass a stub). */
export type RpcServerLike = Pick<
  rpc.Server,
  'getAccount' | 'simulateTransaction' | 'prepareTransaction' | 'sendTransaction' | 'pollTransaction'
>;

export interface SorobanRpcAdapterOptions {
  rpcUrl: string;
  networkPassphrase: string;
  /** Default fee-paying account (`G…`). Overridden per call by `invocation.source`. */
  source: string;
  signTransaction: TransactionSigner;
  /** Polling attempts while waiting for a submitted transaction (1s apart). */
  pollAttempts?: number;
  /** Inject a server (tests); defaults to `new rpc.Server(rpcUrl)`. */
  server?: RpcServerLike;
}

export function toScVal(value: ContractValue): xdr.ScVal {
  if (value === null) return xdr.ScVal.scvVoid();
  if (typeof value === 'string') {
    if (StrKey.isValidEd25519PublicKey(value) || StrKey.isValidContract(value)) {
      return new Address(value).toScVal();
    }
    return nativeToScVal(value, { type: 'string' });
  }
  if (typeof value === 'bigint') return nativeToScVal(value, { type: 'i128' });
  if (typeof value === 'boolean') return xdr.ScVal.scvBool(value);
  if (value instanceof Uint8Array) return xdr.ScVal.scvBytes(Buffer.from(value));
  if (Array.isArray(value)) return xdr.ScVal.scvVec(value.map(toScVal));
  // Soroban structs are maps with symbol keys in sorted order.
  const entries = Object.keys(value)
    .sort()
    .map((key) => new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol(key), val: toScVal(value[key]!) }));
  return xdr.ScVal.scvMap(entries);
}

export class SorobanRpcAdapter implements ContractAdapter {
  private readonly server: RpcServerLike;

  constructor(private readonly options: SorobanRpcAdapterOptions) {
    this.server =
      options.server ?? new rpc.Server(options.rpcUrl, { allowHttp: options.rpcUrl.startsWith('http://') });
  }

  async invoke<T = unknown>(invocation: ContractInvocation): Promise<T> {
    const { networkPassphrase } = this.options;
    const account: Account = await this.server.getAccount(invocation.source ?? this.options.source);
    const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase })
      .addOperation(new Contract(invocation.contractId).call(invocation.method, ...invocation.args.map(toScVal)))
      .setTimeout(30)
      .build();

    if (invocation.readonly) {
      const sim = await this.server.simulateTransaction(tx);
      // The error string carries `Error(Contract, #n)`, which normalizeProtocolError understands.
      if (rpc.Api.isSimulationError(sim)) throw new Error(sim.error);
      return (sim.result ? scValToNative(sim.result.retval) : undefined) as T;
    }

    // Simulates, attaches the footprint/resource fee, and throws on simulation errors.
    const prepared = await this.server.prepareTransaction(tx);
    const signedXdr = await this.options.signTransaction(prepared.toXDR(), { networkPassphrase });
    const signed = TransactionBuilder.fromXDR(signedXdr, networkPassphrase);

    const sent = await this.server.sendTransaction(signed);
    if (sent.status === 'ERROR') {
      throw new Error(`sendTransaction rejected ${sent.hash}: ${sent.errorResult?.toXDR('base64') ?? 'unknown'}`);
    }
    const final = await this.server.pollTransaction(sent.hash, { attempts: this.options.pollAttempts ?? 30 });
    if (final.status !== rpc.Api.GetTransactionStatus.SUCCESS) {
      throw new Error(`transaction ${sent.hash} finished with status ${final.status}`);
    }
    return (final.returnValue ? scValToNative(final.returnValue) : undefined) as T;
  }
}

/** Server-side signer from a secret key (bots, scripts, the devnet). Never ship secrets to browsers. */
export function keypairSigner(secret: string): TransactionSigner {
  const keypair = Keypair.fromSecret(secret);
  return async (txXdr, { networkPassphrase }) => {
    const tx = TransactionBuilder.fromXDR(txXdr, networkPassphrase);
    tx.sign(keypair);
    return tx.toXDR();
  };
}
