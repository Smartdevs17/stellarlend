#!/usr/bin/env node

import process from 'node:process';
import { pathToFileURL } from 'node:url';

function validateHealthUrl(value, label) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error(`${label} must be a valid HTTP(S) URL.`);
  }
  if (!['http:', 'https:'].includes(url.protocol)) {
    throw new Error(`${label} must use HTTP or HTTPS.`);
  }
  if (url.username || url.password || url.search || url.hash) {
    throw new Error(`${label} must not contain credentials, query parameters, or a fragment.`);
  }
  return url;
}

export async function checkFailover({
  primaryUrl,
  secondaryUrl,
  timeoutMs = 5000,
  fetchImpl = fetch,
}) {
  const primary = validateHealthUrl(primaryUrl, 'Primary health URL');
  const secondary = validateHealthUrl(secondaryUrl, 'Failover health URL');
  if (primary.href === secondary.href) {
    throw new Error('Primary and failover health URLs must be different.');
  }
  if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 60_000) {
    throw new Error('Timeout must be an integer between 1 and 60000 milliseconds.');
  }

  const probe = async (name, url) => {
    try {
      const response = await fetchImpl(url, {
        method: 'GET',
        signal: AbortSignal.timeout(timeoutMs),
      });
      return {
        name,
        url: url.href,
        healthy: response.ok,
        status: response.status,
        reason: response.ok ? null : `HTTP ${response.status}`,
      };
    } catch (error) {
      return {
        name,
        url: url.href,
        healthy: false,
        status: null,
        reason: `request failed (${error instanceof Error ? error.name : 'unknown error'})`,
      };
    }
  };

  const checks = await Promise.all([
    probe('primary', primary),
    probe('secondary', secondary),
  ]);
  const selected = checks.find(({ healthy }) => healthy) ?? null;

  return {
    ready: checks.every(({ healthy }) => healthy),
    selected,
    failoverSucceeded: selected?.name === 'secondary',
    checks,
  };
}

function parseArgs(args) {
  const values = new Map();
  for (let index = 0; index < args.length; index += 1) {
    const option = args[index];
    if (!['--primary', '--secondary', '--timeout-ms'].includes(option) || !args[index + 1]) {
      throw new Error(
        'Usage: failover-check.mjs --primary <url> --secondary <url> [--timeout-ms <milliseconds>]',
      );
    }
    values.set(option, args[index + 1]);
    index += 1;
  }
  if (!values.has('--primary') || !values.has('--secondary')) {
    throw new Error(
      'Usage: failover-check.mjs --primary <url> --secondary <url> [--timeout-ms <milliseconds>]',
    );
  }
  return values;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const timeoutMs = Number(args.get('--timeout-ms') ?? 5000);
  const result = await checkFailover({
    primaryUrl: args.get('--primary'),
    secondaryUrl: args.get('--secondary'),
    timeoutMs,
  });

  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  if (!result.checks[1].healthy) {
    process.stderr.write('The failover region is unhealthy; failover check failed.\n');
    process.exitCode = 1;
  } else if (!result.selected) {
    process.stderr.write('Neither region is healthy; failover check failed.\n');
    process.exitCode = 1;
  } else if (result.failoverSucceeded) {
    process.stderr.write('Primary is unhealthy; the secondary endpoint is a healthy failover candidate.\n');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    process.stderr.write(`Failover check failed: ${error.message}\n`);
    process.exitCode = 1;
  });
}
