/**
 * Example: load the local devnet's network settings and contract ids.
 *
 * `npm run devnet` writes `.devnet/state.json`; `npm run devnet:faucet -- <name>` funds
 * extra accounts against it. Every other example takes plain values, so swap this for
 * your own config loader on testnet or mainnet.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

export interface DevnetState {
  network: { rpcUrl: string; networkPassphrase: string; friendbotUrl?: string };
  contracts: { lending: string; xlm: string; usdc: string; lendingWasmHash?: string };
  accounts: Record<string, { identity: string; address: string }>;
}

/** Default location: `<repo>/.devnet/state.json`, overridable with `DEVNET_STATE_DIR`. */
export function devnetStatePath(env: NodeJS.ProcessEnv = process.env): string {
  return resolve(env.DEVNET_STATE_DIR ?? resolve(__dirname, '../../../.devnet'), 'state.json');
}

export function parseDevnetState(json: string): DevnetState {
  const state = JSON.parse(json) as Partial<DevnetState>;
  for (const key of ['lending', 'xlm', 'usdc'] as const) {
    if (!state.contracts?.[key]) throw new Error(`devnet state is missing contracts.${key}`);
  }
  if (!state.network?.rpcUrl || !state.network.networkPassphrase) {
    throw new Error('devnet state is missing network.rpcUrl or network.networkPassphrase');
  }
  return { accounts: {}, ...state } as DevnetState;
}

export function loadDevnetState(path = devnetStatePath()): DevnetState {
  let json: string;
  try {
    json = readFileSync(path, 'utf8');
  } catch {
    throw new Error(`${path} not found; run 'npm run devnet' first`);
  }
  return parseDevnetState(json);
}
