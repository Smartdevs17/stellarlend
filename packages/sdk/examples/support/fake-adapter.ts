import type { ContractAdapter, ContractInvocation } from '../../src';

type Handler = (invocation: ContractInvocation) => unknown;

/**
 * In-memory adapter for tests and demos: records every invocation and answers from
 * per-method handlers (a value, or a function of the invocation; throw to simulate failure).
 */
export class FakeAdapter implements ContractAdapter {
  readonly calls: ContractInvocation[] = [];
  private readonly handlers = new Map<string, Handler>();

  on(method: string, handler: Handler | unknown): this {
    this.handlers.set(method, typeof handler === 'function' ? (handler as Handler) : () => handler);
    return this;
  }

  async invoke<T = unknown>(invocation: ContractInvocation): Promise<T> {
    this.calls.push(invocation);
    const handler = this.handlers.get(invocation.method);
    return (handler ? handler(invocation) : undefined) as T;
  }

  methods(): string[] {
    return this.calls.map((c) => c.method);
  }
}
