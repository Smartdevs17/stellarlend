/**
 * Example: connecting a browser wallet and using it to sign lending transactions.
 *
 * `createInjectedWalletProvider` adapts extensions such as Freighter or xBull without
 * bundling them; `WalletManager` tracks connection state for your UI.
 */
import {
  StellarLendClient,
  WalletManager,
  createInjectedWalletProvider,
  type InjectedWalletApi,
  type WalletState,
} from '../src';
import { SorobanRpcAdapter } from './rpc-adapter';

export interface NetworkConfig {
  rpcUrl: string;
  networkPassphrase: string;
  lendingContractId: string;
}

/** Register an injected wallet. `getApi` returns the extension's API object, or undefined if not installed. */
export function createWalletManager(getApi: () => InjectedWalletApi | undefined): WalletManager {
  return new WalletManager([createInjectedWalletProvider({ id: 'freighter', name: 'Freighter', getApi })]);
}

/**
 * Connect and build a client whose transactions are signed by the connected wallet.
 * `onChange` receives every state transition (connecting → connected, errors, ...).
 */
export async function connectLending(
  wallets: WalletManager,
  network: NetworkConfig,
  onChange: (state: WalletState) => void = () => {},
): Promise<{ client: StellarLendClient; address: string; unsubscribe: () => void }> {
  const unsubscribe = wallets.subscribe(() => onChange(wallets.getState()));
  const { address } = await wallets.connect('freighter');

  const adapter = new SorobanRpcAdapter({
    rpcUrl: network.rpcUrl,
    networkPassphrase: network.networkPassphrase,
    source: address,
    signTransaction: (txXdr, { networkPassphrase }) =>
      wallets.signTransaction(txXdr, { networkPassphrase, accountToSign: address }),
  });

  return {
    client: new StellarLendClient({ contractId: network.lendingContractId, adapter, source: address }),
    address,
    unsubscribe,
  };
}
