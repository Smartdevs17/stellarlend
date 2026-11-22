import type { HealthStatus } from '../types/index.js';

export type { PriceProvider } from './price-provider.js';
import type { PriceProvider } from './price-provider.js';

export type ProviderFactory = () => PriceProvider;

export class ProviderContainer {
  private readonly factories = new Map<string, ProviderFactory>();

  register(name: string, factory: ProviderFactory): this {
    if (this.factories.has(name)) {
      throw new Error(`Provider already registered: ${name}`);
    }
    this.factories.set(name, factory);
    return this;
  }

  resolve(name: string): PriceProvider {
    const factory = this.factories.get(name);
    if (!factory) {
      throw new Error(`Provider not registered: ${name}`);
    }
    return factory();
  }

  resolveChain(names: string[]): PriceProvider[] {
    return names
      .map((name) => this.resolve(name))
      .filter((provider) => provider.isEnabled)
      .sort((a, b) => a.priority - b.priority);
  }

  async healthCheck(names: string[]): Promise<HealthStatus[]> {
    const providers = this.resolveChain(names);
    return Promise.all(providers.map((provider) => provider.healthCheck()));
  }
}

export function createProviderContainer(): ProviderContainer {
  return new ProviderContainer();
}

export function asPriceProvider(provider: PriceProvider): PriceProvider {
  return provider;
}
