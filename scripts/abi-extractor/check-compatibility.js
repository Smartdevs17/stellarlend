#!/usr/bin/env node
'use strict';
const fs = require('fs');
const path = require('path');
const stable = (value) => {
  if (Array.isArray(value)) return `[${value.map(stable).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${stable(value[key])}`).join(',')}}`;
  return JSON.stringify(value);
};
const list = (value) => value == null ? [] : Array.isArray(value) ? value : [value];
const specOf = (abi) => {
  if (Array.isArray(abi)) return abi;
  if (Array.isArray(abi.spec)) return abi.spec;
  if (Array.isArray(abi.contract?.spec)) return abi.contract.spec;
  throw new Error('ABI must be an array or contain a spec array');
};
function unwrap(entry) {
  const kinds = ['function', 'struct', 'union', 'enum', 'error_enum', 'udt_struct', 'udt_union', 'udt_enum', 'udt_error_enum'];
  const kind = entry.type || Object.keys(entry).find((key) => kinds.includes(key));
  return { kind: String(kind || 'unknown').replace(/^udt_/, ''), value: entry[kind] && typeof entry[kind] === 'object' ? entry[kind] : entry };
}
function indexSpec(abi) {
  const index = new Map();
  for (const raw of specOf(abi)) {
    const { kind, value } = unwrap(raw);
    const name = value.name || value.symbol || value.function_name || '<anonymous>';
    index.set(`${kind}:${name}`, { kind, name, value });
  }
  return index;
}
function signature({ kind, value }) {
  if (kind === 'function') return { inputs: list(value.inputs || value.args).map((item) => item.type ?? item), outputs: list(value.outputs || value.output || value.returns).map((item) => item.type ?? item) };
  if (kind === 'struct') return list(value.fields || value.members).map((item) => ({ name: item.name, type: item.type }));
  if (kind === 'union') return list(value.cases || value.variants).map((item) => ({ name: item.name || item.tag, type: item.type ?? item.types ?? item.values ?? null }));
  if (kind === 'enum' || kind === 'error_enum') return list(value.cases || value.variants || value.values).map((item) => ({ name: item.name, value: item.value ?? item.code }));
  return value;
}
function compareAbis(previous, current, contract = 'contract') {
  const before = indexSpec(previous);
  const after = indexSpec(current);
  const changes = [];
  const add = (severity, code, entry, message) => changes.push({ severity, code, contract, entry, message });
  for (const [key, oldEntry] of before) {
    const next = after.get(key);
    if (!next) add('breaking', `removed_${oldEntry.kind}`, oldEntry.name, `Removed ${oldEntry.kind} '${oldEntry.name}'`);
    else if (stable(signature(oldEntry)) !== stable(signature(next))) add('breaking', `changed_${oldEntry.kind}`, oldEntry.name, `Changed ${oldEntry.kind} '${oldEntry.name}' signature`);
  }
  for (const [key, entry] of after) if (!before.has(key)) add('compatible', `added_${entry.kind}`, entry.name, `Added ${entry.kind} '${entry.name}'`);
  return { contract, compatible: !changes.some((change) => change.severity === 'breaking'), changes };
}
const readJson = (file) => JSON.parse(fs.readFileSync(file, 'utf8'));
const jsonFiles = (target) => fs.readdirSync(target).filter((file) => file.endsWith('.json')).sort();
function comparePaths(previousPath, currentPath) {
  const previousIsDir = fs.statSync(previousPath).isDirectory();
  const currentIsDir = fs.statSync(currentPath).isDirectory();
  if (previousIsDir !== currentIsDir) throw new Error('Both inputs must be files or both must be directories');
  if (!previousIsDir) return [compareAbis(readJson(previousPath), readJson(currentPath), path.basename(currentPath, '.json'))];
  const oldFiles = new Set(jsonFiles(previousPath));
  const newFiles = new Set(jsonFiles(currentPath));
  const reports = [];
  for (const file of oldFiles) {
    const contract = path.basename(file, '.json');
    reports.push(newFiles.has(file) ? compareAbis(readJson(path.join(previousPath, file)), readJson(path.join(currentPath, file)), contract) : {
      contract, compatible: false, changes: [{ severity: 'breaking', code: 'removed_contract', contract, entry: file, message: `Removed contract '${contract}'` }],
    });
  }
  for (const file of newFiles) if (!oldFiles.has(file)) {
    const contract = path.basename(file, '.json');
    reports.push({ contract, compatible: true, changes: [{ severity: 'compatible', code: 'added_contract', contract, entry: file, message: `Added contract '${contract}'` }] });
  }
  return reports;
}
function main(argv) {
  const args = argv.slice(2);
  const json = args.includes('--json');
  const positional = args.filter((arg) => arg !== '--json');
  if (positional.length !== 2) {
    console.error('Usage: check-compatibility.js [--json] <previous-abi-or-dir> <current-abi-or-dir>');
    return 1;
  }
  try {
    const contracts = comparePaths(path.resolve(positional[0]), path.resolve(positional[1]));
    const report = { compatible: contracts.every((item) => item.compatible), breakingChanges: contracts.flatMap((item) => item.changes).filter((change) => change.severity === 'breaking').length, contracts };
    if (json) console.log(JSON.stringify(report, null, 2));
    else {
      for (const contract of contracts) {
        console.log(`${contract.compatible ? 'PASS' : 'FAIL'} ${contract.contract}`);
        for (const change of contract.changes) console.log(`  ${change.severity === 'breaking' ? 'BREAKING' : 'COMPATIBLE'}: ${change.message}`);
      }
      console.log(report.compatible ? 'ABI is backward compatible.' : `ABI is incompatible (${report.breakingChanges} breaking change(s)).`);
    }
    return report.compatible ? 0 : 2;
  } catch (error) {
    console.error(`ABI compatibility check failed: ${error.message}`);
    return 1;
  }
}
module.exports = { compareAbis, comparePaths };
if (require.main === module) process.exitCode = main(process.argv);
