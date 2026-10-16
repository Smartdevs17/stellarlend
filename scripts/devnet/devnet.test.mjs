// Plan tests for the devnet scripts. They run each script with --dry-run, which prints
// every command it would execute, and assert on that plan. No network, Docker or
// stellar CLI is needed, so this runs in CI:
//
//   node --test scripts/devnet/devnet.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const here = import.meta.dirname;

function plan(script, ...args) {
  const res = spawnSync('bash', [join(here, script), '--dry-run', ...args], {
    encoding: 'utf8',
    env: { ...process.env, DEVNET_STATE_DIR: mkdtempSync(join(tmpdir(), 'devnet-')) },
  });
  const commands = res.stderr
    .split('\n')
    .filter((l) => l.startsWith('+ '))
    .map((l) => l.slice(2));
  return { status: res.status, stderr: res.stderr, commands };
}

/** Index of the first command containing every fragment, starting at `from`. */
function find(commands, fragments, from = 0) {
  for (let i = from; i < commands.length; i++) {
    if (fragments.every((f) => commands[i].includes(f))) return i;
  }
  return -1;
}

/** Assert the fragment groups appear in order, each matching a later command than the last. */
function assertOrder(commands, steps) {
  let at = -1;
  for (const step of steps) {
    const next = find(commands, step, at + 1);
    assert.notEqual(next, -1, `missing (in order) command containing ${JSON.stringify(step)}`);
    at = next;
  }
}

for (const script of ['bootstrap.sh', 'rehearse-upgrade.sh', 'lib.sh']) {
  test(`${script} parses`, () => {
    assert.equal(spawnSync('bash', ['-n', join(here, script)]).status, 0);
  });
}

test('bootstrap: one command starts node, deploys, seeds and arms the breaker', () => {
  const { status, commands } = plan('bootstrap.sh');
  assert.equal(status, 0);
  assertOrder(commands, [
    ['stellar container start', '--limits unlimited'],
    ['stellar keys generate sl-admin'],
    ['stellar keys generate sl-carol'],
    ['stellar contract build', 'contracts/lending/Cargo.toml'],
    ['stellar contract deploy', 'stellarlend_lending.wasm'],
    ['stellar contract id asset --asset native'],
    ['stellar contract asset deploy', 'USDC:'],
    ['-- initialize ', '--debt_ceiling'],
    ['-- initialize_deposit_settings'],
    ['-- deposit ', 'sl-alice-address'],
    ['-- borrow ', 'sl-alice-address'],
    ['-- borrow ', 'sl-bob-address'],
    ['-- borrow ', 'sl-carol-address'],
    ['-- repay ', 'sl-carol-address'],
    ['-- set_utilization_breaker', 'trip_utilization_bps'],
    ['--send=no', 'get_user_position'],
  ]);
});

test('bootstrap: seeded borrows respect the 150% minimum collateral ratio', () => {
  const { commands } = plan('bootstrap.sh');
  const borrows = commands.filter((c) => c.includes('-- borrow '));
  assert.equal(borrows.length, 3);
  for (const b of borrows) {
    const amount = BigInt(/--amount (\d+)/.exec(b)[1]);
    const collateral = BigInt(/--collateral_amount (\d+)/.exec(b)[1]);
    assert.ok(collateral * 10_000n >= amount * 15_000n, `under-collateralised: ${b}`);
  }
});

test('bootstrap: --reset stops the node first; --no-node and --skip-build skip steps', () => {
  const reset = plan('bootstrap.sh', '--reset');
  assert.equal(reset.status, 0);
  assertOrder(reset.commands, [['stellar container stop'], ['rm -rf'], ['stellar container start']]);

  const external = plan('bootstrap.sh', '--no-node', '--skip-build');
  assert.equal(external.status, 0);
  assert.equal(find(external.commands, ['stellar container']), -1);
  assert.equal(find(external.commands, ['stellar contract build']), -1);
  assert.notEqual(find(external.commands, ['stellar contract deploy']), -1);
});

test('bootstrap: rejects unknown flags', () => {
  const res = spawnSync('bash', [join(here, 'bootstrap.sh'), '--bogus'], { encoding: 'utf8' });
  assert.equal(res.status, 2);
});

test('rehearse-upgrade: upgrade, migrate twice, smoke test, then roll back', () => {
  const { status, commands } = plan('rehearse-upgrade.sh');
  assert.equal(status, 0);
  assertOrder(commands, [
    ['stellar contract fetch', 'onchain-before.wasm'],
    ['cp', 'previous.wasm'],
    ['stellar contract build'],
    ['--send=no', 'get_user_debt'],
    ['stellar contract upload', 'candidate.wasm'],
    ['-- upgrade ', 'sha256:candidate.wasm'],
    ['stellar contract fetch', 'onchain-after-upgrade.wasm'],
    ['-- migrate_pool_state'],
    ['-- migrate_pool_state'],
    ['-- borrow ', 'sl-alice-address'],
    ['-- repay ', 'sl-alice-address'],
    ['stellar contract upload', 'previous.wasm'],
    ['-- upgrade ', 'sha256:previous.wasm'],
    ['stellar contract fetch', 'onchain-after-rollback.wasm'],
  ]);
});

test('rehearse-upgrade: --candidate skips the build', () => {
  const { status, commands } = plan('rehearse-upgrade.sh', '--candidate', '/tmp/candidate.wasm');
  assert.equal(status, 0);
  assert.equal(find(commands, ['stellar contract build']), -1);
  assert.notEqual(find(commands, ['cp /tmp/candidate.wasm']), -1);
});
