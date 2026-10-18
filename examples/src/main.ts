/**
 * Runs the examples read-only against the local devnet:
 *
 *   npm run devnet                       # once
 *   npm run build -w @stellarlend/examples && npm start -w @stellarlend/examples
 *
 * Prints the lending contract's recent events (auto-paginated) and the review screen for
 * a leverage plan built with the typed transaction builders. Nothing is signed or sent.
 */
import { rpc } from '@stellar/stellar-sdk';
import { LendingTransactions, collect } from '@stellarlend/sdk';

import { loadDevnetState } from './devnet';
import { contractEvents, countEventsByName } from './events';
import { buildLeveragePlan, describe } from './review-queue';

const UNIT = 10_000_000n;
/** How far back to scan; quickstart's RPC keeps a short event history. */
const LOOKBACK_LEDGERS = 10_000;

async function main(): Promise<void> {
  const state = loadDevnetState();
  const server = new rpc.Server(state.network.rpcUrl, { allowHttp: state.network.rpcUrl.startsWith('http://') });
  const { sequence } = await server.getLatestLedger();
  const startLedger = Math.max(1, sequence - LOOKBACK_LEDGERS);

  const counts = await countEventsByName(contractEvents(server, state.contracts.lending, startLedger, { pageSize: 100 }));
  console.log(`Lending events since ledger ${startLedger}:`, counts);

  const first = await collect(contractEvents(server, state.contracts.lending, startLedger, { maxItems: 3 }));
  for (const event of first) console.log(`  #${event.ledger} ${String(event.topics[0])} ${event.txHash}`);

  const alice = state.accounts.alice?.address;
  if (alice) {
    const tx = new LendingTransactions({ contractId: state.contracts.lending, source: alice });
    const plan = buildLeveragePlan(tx, {
      user: alice,
      collateralAsset: state.contracts.xlm,
      borrowAsset: state.contracts.usdc,
      collateral: 1_500n * UNIT,
      borrow: 1_000n * UNIT,
    });
    console.log('\nLeverage plan for alice (review only, not submitted):');
    for (const line of plan.map(describe)) console.log(`  ${line}`);
  }
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
