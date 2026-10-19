export type WalletStatus = 'disconnected' | 'connecting' | 'connected' | 'disconnecting';

export interface WalletConnection {
  address: string;
  network?: string;
}

export interface SignTransactionOptions {
  networkPassphrase?: string;
  accountToSign?: string;
}

export interface WalletProvider {
  id: string;
  name: string;
  icon?: string;
  isAvailable(): boolean | Promise<boolean>;
  connect(): Promise<WalletConnection>;
  disconnect?(): Promise<void>;
  signTransaction(xdr: string, options?: SignTransactionOptions): Promise<string>;
}

export interface WalletState {
  status: WalletStatus;
  providerId: string | null;
  address: string | null;
  network: string | null;
  error: Error | null;
}

export interface AvailableWallet {
  id: string;
  name: string;
  icon?: string;
  available: boolean;
}

type Listener = () => void;

const disconnectedState = (): WalletState => ({
  status: 'disconnected',
  providerId: null,
  address: null,
  network: null,
  error: null,
});

const asError = (value: unknown) =>
  value instanceof Error ? value : new Error(typeof value === 'string' ? value : 'Wallet request failed');

/** Coordinates any number of wallet implementations through one stable API. */
export class WalletManager {
  private readonly providers = new Map<string, WalletProvider>();
  private readonly listeners = new Set<Listener>();
  private state: WalletState = disconnectedState();
  private request = 0;

  constructor(providers: WalletProvider[] = []) {
    for (const provider of providers) this.register(provider);
  }

  register(provider: WalletProvider): () => void {
    if (!provider.id) throw new TypeError('Wallet provider id is required');
    if (this.providers.has(provider.id)) throw new Error(`Wallet provider '${provider.id}' is already registered`);
    this.providers.set(provider.id, provider);
    this.emit();
    return () => {
      if (this.state.providerId === provider.id) void this.disconnect();
      this.providers.delete(provider.id);
      this.emit();
    };
  }

  getState = (): WalletState => this.state;

  subscribe = (listener: Listener): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  async getAvailableProviders(): Promise<AvailableWallet[]> {
    return Promise.all([...this.providers.values()].map(async ({ id, name, icon, isAvailable }) => ({
      id, name, icon, available: await isAvailable(),
    })));
  }

  async connect(providerId: string): Promise<WalletConnection> {
    const provider = this.providers.get(providerId);
    if (!provider) throw new Error(`Unknown wallet provider '${providerId}'`);
    if (!(await provider.isAvailable())) throw new Error(`Wallet provider '${provider.name}' is not available`);

    const currentRequest = ++this.request;
    this.setState({ status: 'connecting', providerId, address: null, network: null, error: null });
    try {
      const connection = await provider.connect();
      if (!connection.address) throw new Error('Wallet returned an empty address');
      if (currentRequest === this.request) {
        this.setState({
          status: 'connected',
          providerId,
          address: connection.address,
          network: connection.network ?? null,
          error: null,
        });
      }
      return connection;
    } catch (value) {
      const error = asError(value);
      if (currentRequest === this.request) this.setState({ ...disconnectedState(), error });
      throw error;
    }
  }

  async disconnect(): Promise<void> {
    const provider = this.state.providerId ? this.providers.get(this.state.providerId) : undefined;
    const currentRequest = ++this.request;
    this.setState({ ...this.state, status: 'disconnecting', error: null });
    try {
      await provider?.disconnect?.();
      if (currentRequest === this.request) this.setState(disconnectedState());
    } catch (value) {
      const error = asError(value);
      if (currentRequest === this.request) this.setState({ ...this.state, status: 'connected', error });
      throw error;
    }
  }

  async signTransaction(xdr: string, options?: SignTransactionOptions): Promise<string> {
    if (this.state.status !== 'connected' || !this.state.providerId) throw new Error('Connect a wallet before signing');
    const provider = this.providers.get(this.state.providerId);
    if (!provider) throw new Error('Connected wallet provider is no longer registered');
    return provider.signTransaction(xdr, options);
  }

  private setState(state: WalletState): void {
    this.state = state;
    this.emit();
  }

  private emit(): void {
    for (const listener of this.listeners) listener();
  }
}

export interface InjectedWalletApi {
  connect(): Promise<WalletConnection | string>;
  disconnect?(): Promise<void>;
  signTransaction(xdr: string, options?: SignTransactionOptions): Promise<string | { signedTxXdr: string }>;
}

export interface InjectedWalletProviderOptions {
  id: string;
  name: string;
  icon?: string;
  getApi: () => InjectedWalletApi | undefined;
}

/** Adapts browser extensions such as Freighter or xBull without bundling them. */
export function createInjectedWalletProvider(options: InjectedWalletProviderOptions): WalletProvider {
  const api = () => {
    const value = options.getApi();
    if (!value) throw new Error(`Wallet provider '${options.name}' is not installed`);
    return value;
  };
  return {
    id: options.id,
    name: options.name,
    icon: options.icon,
    isAvailable: () => Boolean(options.getApi()),
    connect: async () => {
      const result = await api().connect();
      return typeof result === 'string' ? { address: result } : result;
    },
    disconnect: async () => { await api().disconnect?.(); },
    signTransaction: async (xdr, signOptions) => {
      const result = await api().signTransaction(xdr, signOptions);
      return typeof result === 'string' ? result : result.signedTxXdr;
    },
  };
}
