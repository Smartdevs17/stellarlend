'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { compareAbis } = require('./check-compatibility');
const base = { spec: [
  { type: 'function', name: 'deposit', inputs: [{ type: 'i128' }], outputs: [{ type: 'bool' }] },
  { type: 'udt_struct', name: 'Position', fields: [{ name: 'debt', type: 'i128' }] },
] };
test('accepts additive entrypoints', () => {
  const result = compareAbis(base, { spec: [...base.spec, { type: 'function', name: 'repay', inputs: [], outputs: [] }] });
  assert.equal(result.compatible, true);
  assert.equal(result.changes[0].code, 'added_function');
});
test('rejects removed entrypoints', () => {
  const result = compareAbis(base, { spec: base.spec.slice(1) });
  assert.equal(result.compatible, false);
  assert.equal(result.changes[0].code, 'removed_function');
});
test('rejects changed parameters and stored types', () => {
  const result = compareAbis(base, { spec: [
    { ...base.spec[0], inputs: [{ type: 'u64' }] },
    { ...base.spec[1], fields: [{ name: 'debt', type: 'u128' }] },
  ] });
  assert.deepEqual(result.changes.map((change) => change.code), ['changed_function', 'changed_struct']);
});
