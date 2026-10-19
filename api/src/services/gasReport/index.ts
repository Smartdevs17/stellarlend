import { loadGasReport, GasReportSourceOptions } from './sources';
import type { GasReport } from './report';

export * from './report';
export { loadGasReport, parseMeasurements, parseHistory } from './sources';

const CACHE_TTL_MS = 60_000;
let cached: { report: GasReport; at: number } | null = null;

/** Report over the repo's benchmark data, cached for a minute. */
export function getGasReport(
  options: GasReportSourceOptions & { refresh?: boolean } = {}
): GasReport {
  if (!options.refresh && cached && Date.now() - cached.at < CACHE_TTL_MS) return cached.report;
  const report = loadGasReport(options);
  cached = { report, at: Date.now() };
  return report;
}

export function clearGasReportCache(): void {
  cached = null;
}
